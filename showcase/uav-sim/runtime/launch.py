#!/usr/bin/env python3
"""Forward termination to the ready UAV Python owner, retaining NVIDIA's wrapper."""
from __future__ import annotations

import os
import selectors
import select
import signal
import subprocess
import threading
import time

_READY_FD = "VEOVEO_UAV_READY_FD"
_ACK_FD = "VEOVEO_UAV_ACK_FD"
_READY_DEADLINE = "VEOVEO_UAV_READY_DEADLINE"
_READY_SECONDS = 10.0
_FAILED_STOP_SECONDS = 1.0
_COMMAND = ("/isaac-sim/python.sh", "/opt/veoveo/uav-sim/entrypoint.py")


def _open_owner(pid: int, wrapper_pid: int) -> int:
    descriptor = os.pidfd_open(pid)
    try:
        # The upstream wrapper launches exactly one direct Python child. Read
        # after opening the pidfd, so reuse cannot redirect a later signal.
        try:
            with open(f"/proc/{pid}/stat", encoding="ascii") as status:
                fields = status.read().rsplit(")", 1)[1].split()
        except FileNotFoundError:
            if select.select([descriptor], [], [], 0)[0]:
                raise ProcessLookupError("UAV Python owner has exited") from None
            raise
        if int(fields[1]) != wrapper_pid:
            raise RuntimeError("UAV Python owner is not the wrapper's child")
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def _retire_failed_launch(wrapper: subprocess.Popen) -> None:
    # This is failed-start disposal, never the normal graceful-stop path.
    try:
        os.killpg(wrapper.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    deadline = time.monotonic() + _FAILED_STOP_SECONDS
    while time.monotonic() < deadline:
        wrapper.poll()
        try:
            os.killpg(wrapper.pid, 0)
        except ProcessLookupError:
            break
        time.sleep(0.05)
    try:
        os.killpg(wrapper.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    reap_deadline = time.monotonic() + _FAILED_STOP_SECONDS
    wrapper.wait(timeout=max(0.0, reap_deadline - time.monotonic()))
    # As container PID 1, reap any children adopted after the shell exits.
    while True:
        try:
            pid, _ = os.waitpid(-1, os.WNOHANG)
        except ChildProcessError:
            break
        if pid == 0:
            if time.monotonic() >= reap_deadline:
                raise TimeoutError("failed UAV startup children did not exit")
            time.sleep(min(0.05, max(0.0, reap_deadline - time.monotonic())))


def _run(command: tuple[str, ...]) -> int:
    if not hasattr(os, "pidfd_open") or not hasattr(signal, "pidfd_send_signal"):
        raise RuntimeError("UAV launcher requires Linux Python pidfd support")
    stop_requested = threading.Event()

    def request_stop(_signum: int, _frame: object) -> None:
        stop_requested.set()

    previous = signal.signal(signal.SIGTERM, request_stop)
    read_fd = write_fd = ack_read = ack_write = None
    wrapper = None
    owner_fd = None
    try:
        read_fd, write_fd = os.pipe()
        ack_read, ack_write = os.pipe()
        environment = os.environ.copy()
        ready_deadline = time.monotonic() + _READY_SECONDS
        environment[_READY_FD] = str(write_fd)
        environment[_ACK_FD] = str(ack_read)
        environment[_READY_DEADLINE] = repr(ready_deadline)
        wrapper = subprocess.Popen(
            command, env=environment, pass_fds=(write_fd, ack_read), start_new_session=True,
        )
        os.close(ack_read)
        ack_read = None
        os.close(write_fd)
        write_fd = None
        message = bytearray()
        signalled = False
        reported = False
        with selectors.DefaultSelector() as ready:
            ready.register(read_fd, selectors.EVENT_READ)
            while True:
                result = wrapper.poll()
                if result is not None and reported:
                    break
                if not reported and time.monotonic() >= ready_deadline:
                    raise RuntimeError("UAV Python owner readiness timed out")
                if owner_fd is not None and stop_requested.is_set() and not signalled:
                    try:
                        signal.pidfd_send_signal(owner_fd, signal.SIGTERM)
                    except ProcessLookupError:
                        pass  # The fenced owner has already exited; reap its wrapper.
                    signalled = True
                if ready.select(0.1):
                    chunk = os.read(read_fd, 64)
                    message.extend(chunk)
                    if len(message) > 32 or (b"\n" in message and not message.endswith(b"\n")):
                        raise RuntimeError("invalid UAV Python owner handshake")
                    if message.endswith(b"\n"):
                        raw = bytes(message[:-1])
                        if not raw.isdigit() or int(raw) <= 1:
                            raise RuntimeError("invalid UAV Python owner handshake")
                        try:
                            owner_fd = _open_owner(int(raw), wrapper.pid)
                        except ProcessLookupError:
                            raise RuntimeError("UAV Python owner exited before admission") from None
                        if stop_requested.is_set():
                            try:
                                signal.pidfd_send_signal(owner_fd, signal.SIGTERM)
                            except ProcessLookupError:
                                raise RuntimeError("UAV Python owner exited before admission") from None
                            signalled = True
                        if time.monotonic() >= ready_deadline:
                            raise RuntimeError("UAV Python owner readiness timed out")
                        os.write(ack_write, b"\x01")
                        os.close(ack_write)
                        ack_write = None
                        reported = True
                        ready.unregister(read_fd)
                    elif not chunk:
                        try:
                            result = wrapper.wait(timeout=min(
                                0.1, max(0.0, ready_deadline - time.monotonic()),
                            ))
                        except subprocess.TimeoutExpired:
                            result = None
                        if result is not None and result != 0:
                            return result if result >= 0 else 128 - result
                        raise RuntimeError("UAV Python owner readiness pipe closed without a report")
        # No signal is sent to the shell: it must observe Python's real outcome
        # through its unchanged wait/error_exit path. Never turn 143 into success.
        result = wrapper.wait()
        return result if result >= 0 else 128 - result
    except BaseException as error:
        if read_fd is not None:
            os.close(read_fd)
            read_fd = None
        if ack_write is not None:
            os.close(ack_write)
            ack_write = None
        if wrapper is not None:
            try:
                _retire_failed_launch(wrapper)
            except BaseException as cleanup_error:
                raise BaseExceptionGroup(
                    "UAV startup and disposal failed", [error, cleanup_error],
                ) from None
        raise
    finally:
        if read_fd is not None:
            os.close(read_fd)
        if write_fd is not None:
            os.close(write_fd)
        if ack_read is not None:
            os.close(ack_read)
        if ack_write is not None:
            os.close(ack_write)
        if owner_fd is not None:
            os.close(owner_fd)
        if wrapper is not None and wrapper.poll() is not None:
            wrapper.wait()
        signal.signal(signal.SIGTERM, previous)


def main() -> int:
    return _run(_COMMAND)


if __name__ == "__main__":
    raise SystemExit(main())
