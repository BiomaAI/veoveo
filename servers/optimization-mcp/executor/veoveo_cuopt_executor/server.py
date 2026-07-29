import asyncio
import logging
import os
from pathlib import Path
from typing import Any

from .gpu import (
    GpuHealth,
    GpuUnavailable,
    assert_gpu_available,
    initialize_gpu,
)
from .protocol import (
    ProtocolError,
    error_response,
    read_frame,
    require_mapping,
    response,
    write_frame,
)

LOGGER = logging.getLogger("veoveo.cuopt.executor")
DEFAULT_SOCKET_PATH = "/run/veoveo-cuopt/executor.sock"
DEFAULT_STAGING_ROOT = "/run/veoveo-cuopt/staging"
DEFAULT_MAX_FRAME_BYTES = 256 * 1024 * 1024


class ExecutorServer:
    def __init__(
        self,
        socket_path: Path,
        staging_root: Path,
        maximum_frame_bytes: int,
        health: GpuHealth,
    ) -> None:
        self.socket_path = socket_path
        self.staging_root = staging_root.resolve()
        self.maximum_frame_bytes = maximum_frame_bytes
        self.health = health
        self.solve_lock = asyncio.Lock()
        self.active_run_id: str | None = None

    async def serve(self) -> None:
        self.socket_path.parent.mkdir(mode=0o750, parents=True, exist_ok=True)
        self.staging_root.mkdir(mode=0o750, parents=True, exist_ok=True)
        if self.socket_path.exists():
            if not self.socket_path.is_socket():
                raise RuntimeError(
                    f"refusing to replace non-socket path {self.socket_path}"
                )
            self.socket_path.unlink()
        server = await asyncio.start_unix_server(
            self.handle_connection, path=self.socket_path
        )
        self.socket_path.chmod(0o660)
        LOGGER.info(
            "cuOpt executor ready on %s with GPU %s",
            self.socket_path,
            self.health.gpu_uuid,
        )
        async with server:
            await server.serve_forever()

    async def handle_connection(
        self,
        reader: asyncio.StreamReader,
        writer: asyncio.StreamWriter,
    ) -> None:
        run_id = "run-00000000-0000-7000-8000-000000000000"
        try:
            request = await read_frame(reader, self.maximum_frame_bytes)
            run_id = request["run_id"]
            operation = require_mapping(request["operation"], "operation")
            operation_name = operation["operation"]
            if operation_name == "health":
                assert_gpu_available()
                result = {
                    "result": "health",
                    "health": self.health.to_dict(),
                }
            elif operation_name == "cancel":
                target = operation.get("target_run_id")
                if target == self.active_run_id:
                    LOGGER.warning(
                        "cancelling active run %s by terminating the worker",
                        target,
                    )
                    writer.close()
                    await writer.wait_closed()
                    os._exit(75)
                result = {
                    "result": "error",
                    "error": {
                        "code": "invalid_request",
                        "message": f"run {target} is not active",
                        "findings": [],
                    },
                }
            else:
                result = await self.solve(request, operation)
            await write_frame(
                writer,
                response(run_id, result),
                self.maximum_frame_bytes,
            )
        except ProtocolError as error:
            await self._write_error(
                writer, run_id, "protocol_failure", str(error)
            )
        except GpuUnavailable as error:
            await self._write_error(
                writer, run_id, "gpu_unavailable", str(error)
            )
        except MemoryError as error:
            await self._write_error(
                writer, run_id, "out_of_memory", str(error)
            )
        except Exception as error:
            LOGGER.exception("cuOpt executor request failed")
            await self._write_error(
                writer, run_id, "solver_failure", str(error)
            )
        finally:
            if not writer.is_closing():
                writer.close()
                await writer.wait_closed()

    async def solve(
        self, request: dict[str, Any], operation: dict[str, Any]
    ) -> dict[str, Any]:
        if request.get("profile") is None:
            raise ProtocolError("solver operation requires a profile")
        if self.solve_lock.locked():
            raise ProtocolError(
                "executor already has an active solve; queue in the control plane"
            )
        async with self.solve_lock:
            self.active_run_id = request["run_id"]
            try:
                return await asyncio.to_thread(
                    dispatch, operation, request["profile"], self.staging_root
                )
            finally:
                self.active_run_id = None

    async def _write_error(
        self,
        writer: asyncio.StreamWriter,
        run_id: str,
        code: str,
        message: str,
    ) -> None:
        if writer.is_closing():
            return
        try:
            await write_frame(
                writer,
                error_response(run_id, code, message),
                self.maximum_frame_bytes,
            )
        except (ConnectionError, ProtocolError):
            pass


def dispatch(
    operation: dict[str, Any],
    profile: dict[str, Any],
    staging_root: Path,
) -> dict[str, Any]:
    name = operation["operation"]
    if name == "solve_routes":
        from .routing import solve_routes

        return {
            "result": "routes",
            "solution": solve_routes(
                require_mapping(operation["problem"], "problem"),
                require_mapping(profile["routing"], "profile.routing"),
            ),
        }
    if name == "solve_route_scenarios":
        from .routing import solve_route_scenarios

        cases = operation.get("cases")
        if not isinstance(cases, list) or not cases:
            raise ProtocolError("route scenarios require non-empty cases")
        return {
            "result": "route_scenarios",
            "solutions": solve_route_scenarios(
                cases,
                require_mapping(profile["routing"], "profile.routing"),
            ),
        }
    if name == "solve_model":
        from .mathematical import solve_model

        family = operation["family"]
        return {
            "result": "model",
            "solution": solve_model(
                family,
                require_mapping(operation["model"], "model"),
                profile,
            ),
        }
    if name == "solve_model_file":
        from .mathematical import solve_model_file

        family = operation["family"]
        path = _staged_path(operation["staged_path"], staging_root)
        return {
            "result": "model",
            "solution": solve_model_file(family, str(path), profile),
        }
    raise ProtocolError(f"unsupported operation {name}")


def _staged_path(value: Any, staging_root: Path) -> Path:
    if not isinstance(value, str) or not value:
        raise ProtocolError("staged_path must be a string")
    path = Path(value).resolve(strict=True)
    if path == staging_root or staging_root not in path.parents:
        raise ProtocolError(
            f"staged_path must be beneath {staging_root}"
        )
    if not path.is_file():
        raise ProtocolError("staged_path must identify a regular file")
    return path


def main() -> None:
    logging.basicConfig(
        level=os.environ.get("VEOVEO_CUOPT_LOG_LEVEL", "INFO"),
        format="%(asctime)s %(levelname)s %(name)s %(message)s",
    )
    socket_path = Path(
        os.environ.get("VEOVEO_CUOPT_SOCKET", DEFAULT_SOCKET_PATH)
    )
    staging_root = Path(
        os.environ.get("VEOVEO_CUOPT_STAGING_ROOT", DEFAULT_STAGING_ROOT)
    )
    maximum_frame_bytes = int(
        os.environ.get(
            "VEOVEO_CUOPT_MAX_FRAME_BYTES",
            str(DEFAULT_MAX_FRAME_BYTES),
        )
    )
    if maximum_frame_bytes <= 0:
        raise SystemExit("VEOVEO_CUOPT_MAX_FRAME_BYTES must be positive")
    try:
        health = initialize_gpu()
    except GpuUnavailable as error:
        LOGGER.critical("GPU initialization failed: %s", error)
        raise SystemExit(78) from error
    server = ExecutorServer(
        socket_path, staging_root, maximum_frame_bytes, health
    )
    try:
        asyncio.run(server.serve())
    finally:
        if socket_path.exists() and socket_path.is_socket():
            socket_path.unlink()
