import json
import os
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


@pytest.fixture(scope="session")
def surreal_platform():
    """Fresh kernel lanes installed by the real Gateway composition commands."""
    if not _docker_available():
        pytest.fail("docker is required for platform-store integration tests")
    gateway = _gateway_binary()
    port = _free_port()
    name = f"veoveo-pytest-surreal-{uuid.uuid4().hex[:12]}"
    try:
        subprocess.run(
            [
                "docker",
                "run",
                "-d",
                "--rm",
                "--cpus=2",
                "--memory=2g",
                "--name",
                name,
                "-p",
                f"127.0.0.1:{port}:8000",
                SURREAL_IMAGE,
                "start",
                "--log",
                "warn",
                "--user",
                "root",
                "--pass",
                "root",
                "memory",
            ],
            check=True,
            capture_output=True,
            timeout=60,
        )
        endpoint = f"ws://127.0.0.1:{port}"
        _wait_ready(port)
        namespace, database = "veoveo_pytest", "platform"
        _install_kernel_lanes(gateway, endpoint, namespace, database)
        yield {
            "endpoint": endpoint,
            "namespace": namespace,
            "database": database,
            "username": RUNTIME_USER,
            "password": RUNTIME_PASSWORD,
        }
    finally:
        subprocess.run(["docker", "rm", "--force", name], check=False, capture_output=True, timeout=30)


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
