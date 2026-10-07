import json
import os
import re
import tempfile
import shutil
import socket
import subprocess
import time
import uuid
from pathlib import Path

import pytest

SURREAL_IMAGE = "surrealdb/surrealdb@sha256:681c6c22c287421b5c7d99e0fde79b6e0d32c36c1ddeaab2762a1661cb04cd20"
RUNTIME_USER = "veoveo_runtime"
RUNTIME_PASSWORD = "runtime-secret"


def _free_port() -> int:
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        return listener.getsockname()[1]


def _docker_available() -> bool:
    return shutil.which("docker") is not None


def _redacted_output(value: str | bytes | None) -> str:
    if isinstance(value, bytes):
        value = value.decode("utf-8", errors="replace")
    return (value or "").replace(RUNTIME_PASSWORD, "[REDACTED]")[-4096:]


def _fixture_cid(path: Path) -> str | None:
    try:
        value = path.read_text(encoding="utf-8").strip()
    except FileNotFoundError:
        return None
    if re.fullmatch(r"[0-9a-f]{64}", value) is None:
        raise RuntimeError("Docker fixture CID is invalid; refuse name-based cleanup")
    return value


def _remove_owned_surreal(cid: str) -> None:
    if re.fullmatch(r"[0-9a-f]{64}", cid) is None:
        raise RuntimeError("Docker fixture removal requires its admitted CID")
    try:
        result = subprocess.run(
            ["docker", "rm", "--force", "--volumes", cid],
            check=False, capture_output=True, text=True, timeout=30,
        )
    except subprocess.TimeoutExpired:
        raise RuntimeError("Owned SurrealDB removal exceeded 30 seconds; identity retained") from None
    if result.returncode or result.stdout.strip() != cid:
        raise RuntimeError(
            "Owned SurrealDB removal is unresolved: "
            + _redacted_output(result.stderr + result.stdout)
        )


@pytest.fixture(scope="session")
def surreal_platform():
    """Fresh kernel lanes installed by the real Gateway composition commands."""
    if not _docker_available():
        pytest.fail("docker is required for platform-store integration tests")
    gateway = _gateway_binary()
    port = _free_port()
    name = f"veoveo-pytest-surreal-{uuid.uuid4().hex[:12]}"
    directory = Path(tempfile.mkdtemp(prefix="veoveo-python-surreal-"))
    directory.chmod(0o700)
    cidfile = directory / "created.cid"
    receipt = {"name": name, "image": SURREAL_IMAGE, "launch": "intent", "cleanup": "pending"}
    receipt_path = directory / "receipt.json"

    def retain() -> None:
        receipt_path.write_text(json.dumps(receipt, indent=2), encoding="utf-8")
        receipt_path.chmod(0o600)

    retain()  # Publish private dispatch intent before Docker may create a container.
    launched = False
    try:
        try:
            result = subprocess.run(
                [
                    "docker", "run", "-d", "--cidfile", str(cidfile),
                    "--cpus=2", "--memory=2g", "--name", name,
                    "-p", f"127.0.0.1:{port}:8000", SURREAL_IMAGE,
                    "start", "--log", "warn", "--user", "root", "--pass", "root", "memory",
                ],
                check=False, capture_output=True, text=True, timeout=60,
            )
        except subprocess.TimeoutExpired as error:
            receipt.update(launch="unknown", diagnostic=_redacted_output(error.stderr))
            retain()
            raise RuntimeError(f"SurrealDB launch exceeded 60 seconds; receipt: {receipt_path}") from None
        except OSError as error:
            receipt.update(launch="unknown", diagnostic=_redacted_output(str(error)))
            retain()
            raise RuntimeError(f"SurrealDB launch failed; receipt: {receipt_path}") from None
        receipt.update(
            launch="returned", returncode=result.returncode,
            diagnostic=_redacted_output(result.stderr + result.stdout),
        )
        retain()
        cid = _fixture_cid(cidfile)
        if result.returncode or cid is None or result.stdout.strip() != cid:
            raise RuntimeError(f"SurrealDB launch identity was not confirmed; receipt: {receipt_path}")
        receipt.update(launch="admitted", cid=cid)
        retain()
        launched = True
        endpoint = f"ws://127.0.0.1:{port}"
        setup_stage = "readiness"
        try:
            _wait_ready(port)
            namespace, database = "veoveo_pytest", "platform"
            setup_stage = "kernel_installation"
            _install_kernel_lanes(gateway, endpoint, namespace, database)
        except Exception as error:
            receipt.update(
                setupStage=setup_stage,
                setupDiagnostic=_redacted_output(f"{type(error).__name__}: {error}"),
            )
            retain()
            raise RuntimeError(f"SurrealDB {setup_stage} failed; receipt: {receipt_path}") from None
        yield {
            "endpoint": endpoint,
            "namespace": namespace,
            "database": database,
            "username": RUNTIME_USER,
            "password": RUNTIME_PASSWORD,
        }
    finally:
        # A failed or timed-out launch may still have created this invocation's CID.
        # Missing or malformed CID keeps the receipt and never authorizes name deletion.
        try:
            cid = _fixture_cid(cidfile)
            if cid is None:
                raise RuntimeError("Docker launch outcome unknown; no admitted fixture CID")
            receipt["cid"] = cid
            retain()
            _remove_owned_surreal(cid)
            receipt["cleanup"] = "settled"
            retain()
        except (OSError, RuntimeError) as error:
            receipt.update(cleanup="unresolved", cleanupDiagnostic=_redacted_output(str(error)))
            retain()
            raise RuntimeError(f"SurrealDB fixture cleanup unresolved; receipt: {receipt_path}") from None
        if launched and "setupDiagnostic" not in receipt:
            shutil.rmtree(directory)
        else:
            # Preserve primary setup diagnostics even after owned cleanup settles.
            print(f"SurrealDB failed-setup receipt: {receipt_path}")


def _wait_ready(port: int) -> None:
    import urllib.request

    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        try:
            with urllib.request.urlopen(f"http://127.0.0.1:{port}/ready", timeout=2):
                return
        except OSError:
            time.sleep(0.3)
    raise RuntimeError("SurrealDB container did not become ready")


def _gateway_binary() -> Path:
    default = Path(__file__).resolve().parents[3] / "target/debug/gateway"
    binary = Path(os.environ.get("VEOVEO_TEST_GATEWAY_BIN", default)).resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        pytest.fail(
            "Python native fixtures require a built Gateway composition binary; "
            "build veoveo-gateway or set VEOVEO_TEST_GATEWAY_BIN to its executable"
        )
    return binary


def _install_kernel_lanes(
    gateway: Path, endpoint: str, namespace: str, database: str,
) -> None:
    # Commands receive no installation environment or configuration search path.
    env = {key: os.environ[key] for key in ("LD_LIBRARY_PATH",) if key in os.environ}
    composition = "sha256:" + "1" * 64
    deadline = time.monotonic() + 300
    with tempfile.TemporaryDirectory(prefix="veoveo-python-module-plan-") as temporary:
        directory = Path(temporary)
        selection = directory / "selection.json"
        plan_path = directory / "plan.json"
        selection.write_text(json.dumps({
            "format": "veoveo.ai/module-selection/v1", "enabled": [],
            "generation": "1", "credentialRevision": "python-fixture-v1",
        }), encoding="utf-8")

        def command(*arguments: str) -> str:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise RuntimeError("Python kernel installation exceeded 300 seconds")
            try:
                result = subprocess.run(
                    [str(gateway), *arguments], cwd=directory, env=env,
                    capture_output=True, text=True, timeout=min(120, remaining),
                )
            except subprocess.TimeoutExpired:
                raise RuntimeError(f"Gateway {arguments[0]} exceeded its fixture deadline") from None
            if result.returncode:
                diagnostic = result.stderr.replace(RUNTIME_PASSWORD, "[REDACTED]")
                raise RuntimeError(f"Gateway {arguments[0]} failed: {diagnostic[-4096:]}")
            return result.stdout

        generated = command(
            "module-plan", "--modules", str(selection), "--composition", composition,
        )
        plan = json.loads(generated)
        plan_path.write_text(generated, encoding="utf-8")
        env.update({
            "VEOVEO_SURREAL_ENDPOINT": endpoint,
            "VEOVEO_SURREAL_NAMESPACE": namespace,
            "VEOVEO_SURREAL_DATABASE": database,
            "VEOVEO_SURREAL_AUTH_LEVEL": "root",
            "VEOVEO_SURREAL_USERNAME": "root", "VEOVEO_SURREAL_PASSWORD": "root",
            "VEOVEO_SURREAL_RUNTIME_USERNAME": RUNTIME_USER,
            "VEOVEO_SURREAL_RUNTIME_PASSWORD": RUNTIME_PASSWORD,
            "VEOVEO_MODULE_PLAN": str(plan_path), "VEOVEO_MODULE_COMPOSITION": composition,
            "VEOVEO_INSTALLATION_GENERATION": "1",
            "VEOVEO_CREDENTIAL_REVISION": "python-fixture-v1",
        })
        command("installation-prepare")
        for lane in plan["lanes"]:
            command("module-migrate", "--module", lane["module"], "--wait-seconds", "0")
