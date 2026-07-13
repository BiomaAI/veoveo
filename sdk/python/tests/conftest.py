import asyncio
import shutil
import socket
import subprocess
import time
import uuid
from pathlib import Path

import pytest
from surrealdb import AsyncSurreal

MIGRATIONS_DIR = (
    Path(__file__).resolve().parents[3] / "crates" / "platform-store" / "migrations"
)
SURREAL_IMAGE = "surrealdb/surrealdb:v3.2.0"
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
    """A SurrealDB v3.2.0 container with the real platform migrations applied."""
    if not _docker_available():
        pytest.skip("docker is required for platform-store integration tests")
    if not MIGRATIONS_DIR.is_dir():
        pytest.skip(f"platform migrations not found at {MIGRATIONS_DIR}")
    port = _free_port()
    name = f"veoveo-pytest-surreal-{uuid.uuid4().hex[:12]}"
    subprocess.run(
        [
            "docker",
            "run",
            "-d",
            "--rm",
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
    )
    endpoint = f"ws://127.0.0.1:{port}"
    try:
        _wait_ready(port)
        namespace, database = "veoveo_pytest", "platform"
        asyncio.run(_apply_migrations(endpoint, namespace, database))
        yield {
            "endpoint": endpoint,
            "namespace": namespace,
            "database": database,
            "username": RUNTIME_USER,
            "password": RUNTIME_PASSWORD,
        }
    finally:
        subprocess.run(["docker", "stop", name], check=False, capture_output=True)


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


async def _apply_migrations(endpoint: str, namespace: str, database: str) -> None:
    db = AsyncSurreal(endpoint)
    await db.signin({"username": "root", "password": "root"})
    await db.use(namespace, database)
    for path in sorted(MIGRATIONS_DIR.glob("*.surql")):
        response = await db.query_raw(path.read_text())
        errors = [
            statement.get("result")
            for statement in response.get("result", [])
            if statement.get("status") == "ERR"
        ]
        if errors:
            raise RuntimeError(f"migration {path.name} failed: {errors}")
    await db.query_raw(
        f"DEFINE USER {RUNTIME_USER} ON DATABASE PASSWORD '{RUNTIME_PASSWORD}' "
        "ROLES EDITOR;"
    )
    await db.close()
