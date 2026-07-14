from __future__ import annotations

from pathlib import Path
import os
import signal
import subprocess


class ToolFailure(RuntimeError):
    pass


def run_tool(
    executable: str,
    arguments: list[str],
    *,
    timeout_seconds: int,
    cwd: Path,
    maximum_log_bytes: int = 1_048_576,
) -> subprocess.CompletedProcess[bytes]:
    environment = {
        "PATH": os.environ.get("PATH", "/usr/local/bin:/usr/bin:/bin"),
        "HOME": str(cwd),
        "TMPDIR": str(cwd),
        "PROJ_NETWORK": "OFF",
        "CPL_VSIL_CURL_ALLOWED_EXTENSIONS": "",
    }
    process = subprocess.Popen(
        [executable, *arguments],
        cwd=cwd,
        env=environment,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    try:
        stdout, stderr = process.communicate(timeout=timeout_seconds)
    except subprocess.TimeoutExpired as error:
        terminate_group(process)
        stdout, stderr = process.communicate()
        raise ToolFailure(f"{executable} exceeded its time limit") from error
    if len(stdout) + len(stderr) > maximum_log_bytes:
        raise ToolFailure(f"{executable} exceeded its diagnostic-output limit")
    if process.returncode != 0:
        diagnostic = stderr.decode("utf-8", errors="replace")[-4096:]
        raise ToolFailure(f"{executable} failed: {diagnostic}")
    return subprocess.CompletedProcess(process.args, process.returncode, stdout, stderr)


def terminate_group(process: subprocess.Popen[bytes]) -> None:
    if process.poll() is not None:
        return
    try:
        os.killpg(process.pid, signal.SIGTERM)
        process.wait(timeout=5)
    except (ProcessLookupError, subprocess.TimeoutExpired):
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
