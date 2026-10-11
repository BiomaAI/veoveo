from __future__ import annotations

import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import unittest


ENTRYPOINT = Path(__file__).resolve().parents[1] / "entrypoint.py"


class RuntimeShutdownTests(unittest.TestCase):
    def test_sigterm_requests_owner_stop_and_runs_ordered_cleanup(self):
        # Exercise the actual first-party entrypoint in a separate process. The
        # finite stand-in owns no Isaac/GPU resources and makes no native claim.
        program = r'''
import runpy, sys, threading, types
package = types.ModuleType("veoveo_uav_sim")
package.RuntimeConfig = types.SimpleNamespace(from_environment=lambda: object())
app = types.ModuleType("veoveo_uav_sim.app")
def run(config, stop_requested=None):
    try:
        print("owner-running", flush=True)
        while stop_requested is None or not stop_requested.is_set():
            threading.Event().wait(0.1)
    finally:
        print("admission-closed", flush=True)
        print("resources-closed", flush=True)
app.run = run
sys.modules["veoveo_uav_sim"] = package
sys.modules["veoveo_uav_sim.app"] = app
runpy.run_path(sys.argv[1], run_name="__main__")
'''
        child = subprocess.Popen(
            [sys.executable, "-c", program, str(ENTRYPOINT)],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        try:
            with selectors.DefaultSelector() as ready:
                ready.register(child.stdout, selectors.EVENT_READ)
                self.assertTrue(ready.select(3.0), "owner did not start")
                self.assertEqual(child.stdout.readline().strip(), "owner-running")
            os.kill(child.pid, signal.SIGTERM)
            stdout, stderr = child.communicate(timeout=3.0)
            self.assertEqual(child.returncode, 0, stderr)
            self.assertEqual(stdout.splitlines(), ["admission-closed", "resources-closed"])
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate(timeout=3.0)

    @staticmethod
    def configuration():
        from types import SimpleNamespace
        return SimpleNamespace(
            world_bootstrap_file=None, session_id="fixture-session",
            camera=SimpleNamespace(vehicle_id="uav-1", width=640, height=480),
            extension_directory="/fixture/extensions", cache_directory=Path("/fixture/cache"),
            operator_live_view=SimpleNamespace(streamable_cameras=()), vehicle_count=1,
        )

    def test_stop_during_constructor_closes_acquired_app_without_native_imports(self):
        import threading
        from types import ModuleType, SimpleNamespace
        from unittest.mock import Mock, patch
        from veoveo_uav_sim.app import run

        stop = threading.Event()
        close = Mock()
        isaac = ModuleType("isaacsim")

        def construct(_options):
            stop.set()
            return SimpleNamespace(close=close)

        isaac.SimulationApp = construct
        with patch.dict(sys.modules, {"isaacsim": isaac}):
            run(self.configuration(), stop)
        close.assert_called_once_with(exit_code=0)

    def test_initialization_and_cleanup_failures_both_survive(self):
        import threading
        from types import ModuleType, SimpleNamespace
        from unittest.mock import Mock, patch
        from veoveo_uav_sim.app import run

        close_failure = RuntimeError("fixture native close failed")
        close = Mock(side_effect=close_failure)
        isaac = ModuleType("isaacsim")
        isaac.SimulationApp = lambda _options: SimpleNamespace(close=close)
        # No native module tree exists in this CPU fixture: the actual import
        # after acquisition fails, then the acquired app's close also fails.
        with patch.dict(sys.modules, {"isaacsim": isaac}), self.assertLogs("veoveo.uav_sim", level="ERROR"):
            with self.assertRaises(BaseExceptionGroup) as observed:
                run(self.configuration(), threading.Event())
        self.assertIsInstance(observed.exception.exceptions[0], ModuleNotFoundError)
        self.assertIs(observed.exception.exceptions[1], close_failure)
        close.assert_called_once_with(exit_code=1)

    def test_queued_action_after_stop_is_refused_before_effect(self):
        import threading
        from unittest.mock import Mock
        from veoveo_uav_sim.app import _admit_action
        from veoveo_uav_sim.command_queue import MainThreadQueue

        stop = threading.Event()
        queued = threading.Event()
        action = Mock()
        queue = MainThreadQueue()
        failures = []
        original_put = queue._queue.put

        def observe_put(call):
            original_put(call)
            queued.set()

        queue._queue.put = observe_put

        def submit():
            try:
                queue.submit(lambda: _admit_action(stop, action), timeout_seconds=2.0)
            except RuntimeError as error:
                failures.append(error)

        waiter = threading.Thread(target=submit)
        waiter.start()
        try:
            self.assertTrue(queued.wait(2.0))
            stop.set()
            queue.drain()
            waiter.join(2.0)
            self.assertFalse(waiter.is_alive())
            self.assertEqual(len(failures), 1)
            action.assert_not_called()
        finally:
            stop.set()
            queue.drain()
            waiter.join(2.0)

    def test_launcher_retains_early_stop_waits_wrapper_and_preserves_failure(self):
        import tempfile
        # The installed image uses its existing Kit Python 3.12 with pidfds.
        # Host system Python qualifies these stdlib process mechanics; it does
        # not establish the image's bootstrap or native cleanup behavior.
        launch = ENTRYPOINT.with_name("launch.py")
        owner = r'''
import os, runpy, sys, threading, types
package = types.ModuleType("veoveo_uav_sim")
package.RuntimeConfig = types.SimpleNamespace(from_environment=lambda: object())
app = types.ModuleType("veoveo_uav_sim.app")
def run(config, stop_requested):
    print("owner-running", flush=True)
    if os.environ["FIXTURE_MODE"] == "normal":
        return
    try:
        while not stop_requested.is_set():
            threading.Event().wait(0.1)
    finally:
        print("admission-closed", flush=True)
        print("resources-closed", flush=True)
        if os.environ["FIXTURE_MODE"] == "failed":
            raise RuntimeError("fixture cleanup failed")
app.run = run
sys.modules["veoveo_uav_sim"] = package
sys.modules["veoveo_uav_sim.app"] = app
runpy.run_path(sys.argv[1], run_name="__main__")
'''
        program = r'''
import importlib.util, os, signal, sys
spec = importlib.util.spec_from_file_location("uav_launch", sys.argv[1])
launch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launch)
if os.environ["FIXTURE_MODE"] == "early":
    original = launch.subprocess.Popen
    def start(*args, **kwargs):
        os.kill(os.getpid(), signal.SIGTERM)
        return original(*args, **kwargs)
    launch.subprocess.Popen = start
raise SystemExit(launch._run(("/bin/sh", sys.argv[2], sys.executable, "-c", sys.argv[3], sys.argv[4])))
'''
        with tempfile.TemporaryDirectory() as directory:
            wrapper = Path(directory) / "python.sh"
            # Mirror the upstream spawn/wait/error_exit shape, without vendor
            # environment setup or Isaac execution. A signal to this shell fails.
            wrapper.write_text('trap "exit 99" TERM\nerror_exit() { exit 1; }\n"$@" || error_exit\nprintf "wrapper-reaped\\n"\n')
            for mode in ("early", "normal", "stop", "failed"):
                with self.subTest(mode=mode):
                    environment = os.environ.copy()
                    environment["FIXTURE_MODE"] = mode
                    child = subprocess.Popen(
                        ["/usr/bin/python3", "-c", program, str(launch), str(wrapper), owner, str(ENTRYPOINT)],
                        stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=environment,
                    )
                    try:
                        first = ""
                        if mode in ("stop", "failed"):
                            with selectors.DefaultSelector() as ready:
                                ready.register(child.stdout, selectors.EVENT_READ)
                                self.assertTrue(ready.select(3.0))
                                first = child.stdout.readline()
                                self.assertEqual(first.strip(), "owner-running")
                            child.send_signal(signal.SIGTERM)
                        stdout, stderr = child.communicate(timeout=4.0)
                        lines = (first + stdout).splitlines()
                        self.assertEqual(child.returncode, 1 if mode == "failed" else 0, stderr)
                        if mode != "failed":
                            self.assertEqual(lines[-1], "wrapper-reaped")
                        if mode in ("stop", "failed"):
                            self.assertEqual(lines[:3], ["owner-running", "admission-closed", "resources-closed"])
                    finally:
                        if child.poll() is None:
                            child.kill()
                            child.communicate(timeout=3.0)

    def test_launcher_refuses_a_pid_outside_its_owned_wrapper(self):
        program = r'''
import importlib.util, os, subprocess, sys
spec = importlib.util.spec_from_file_location("uav_launch", sys.argv[1])
launch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launch)
child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(3)"])
try:
    try:
        launch._open_owner(child.pid, os.getpid() + 1)
    except RuntimeError:
        assert child.poll() is None
    else:
        raise AssertionError("foreign parent admitted")
finally:
    child.terminate()
    child.wait(timeout=3.0)
'''
        result = subprocess.run(
            ["/usr/bin/python3", "-c", program, str(ENTRYPOINT.with_name("launch.py"))],
            capture_output=True, text=True, timeout=5.0,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_stop_retained_during_isaac_import_skips_constructor(self):
        import threading
        from types import ModuleType
        from unittest.mock import Mock, patch
        from veoveo_uav_sim.app import run
        stop = threading.Event()
        construct = Mock()
        isaac = ModuleType("isaacsim")

        def importing(name):
            if name == "SimulationApp":
                stop.set()
                return construct
            raise AttributeError(name)

        isaac.__getattr__ = importing
        with patch.dict(sys.modules, {"isaacsim": isaac}):
            run(self.configuration(), stop)
        construct.assert_not_called()

    def test_launcher_disposes_live_unready_or_malformed_owner(self):
        import tempfile
        program = r'''
import importlib.util, sys
spec = importlib.util.spec_from_file_location("uav_launch", sys.argv[1])
launch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launch)
launch._READY_SECONDS = 0.25
launch._FAILED_STOP_SECONDS = 0.25
raise SystemExit(launch._run(("/bin/sh", sys.argv[2], sys.executable, "-c", sys.argv[3], sys.argv[4])))
'''
        owner = r'''
import os, sys, threading
fd = int(os.environ["VEOVEO_UAV_READY_FD"])
print("owner-live:" + str(os.getpid()), flush=True)
if sys.argv[1] == "malformed":
    os.write(fd, b"not-a-pid\n")
elif sys.argv[1] == "foreign":
    os.write(fd, (str(os.getppid()) + "\n").encode("ascii"))
elif sys.argv[1] == "gone":
    os.write(fd, b"1999999999\n")
elif sys.argv[1] == "eof":
    os.close(fd)
threading.Event().wait(10)
'''
        with tempfile.TemporaryDirectory() as directory:
            wrapper = Path(directory) / "python.sh"
            # For failed-start fault injection only, release the shell's copy
            # of the report descriptor while it still owns/waits its child.
            # This makes child EOF observable before the wrapper exits.
            wrapper.write_text('"$@" &\nchild=$!\neval "exec ${VEOVEO_UAV_READY_FD}>&-"\nwait "$child" || exit 1\n')
            for mode in ("deadline", "eof", "malformed", "foreign", "gone"):
                with self.subTest(mode=mode):
                    result = subprocess.run(
                        ["/usr/bin/python3", "-c", program, str(ENTRYPOINT.with_name("launch.py")), str(wrapper), owner, mode],
                        capture_output=True, text=True, timeout=4.0,
                    )
                    self.assertNotEqual(result.returncode, 0)
                    self.assertIn("RuntimeError", result.stderr)
                    if mode == "eof":
                        self.assertIn("readiness pipe closed without a report", result.stderr)
                    elif mode == "deadline":
                        self.assertIn("readiness timed out", result.stderr)
                    pid = int(result.stdout.strip().removeprefix("owner-live:"))
                    status = Path(f"/proc/{pid}/stat")
                    if status.exists():
                        # A non-PID1 test launcher cannot adopt grandchildren;
                        # a zombie is terminal and is reaped by the real init.
                        self.assertEqual(status.read_text().rsplit(")", 1)[1].split()[0], "Z")

    def test_wrapper_failure_before_owner_report_preserves_nonzero_status(self):
        program = r'''
import importlib.util, sys
spec = importlib.util.spec_from_file_location("uav_launch", sys.argv[1])
launch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launch)
raise SystemExit(launch._run(("/bin/sh", "-c", "exit 7")))
'''
        result = subprocess.run(
            ["/usr/bin/python3", "-c", program, str(ENTRYPOINT.with_name("launch.py"))],
            capture_output=True, text=True, timeout=3.0,
        )
        self.assertEqual(result.returncode, 7, result.stderr)

    def test_entrypoint_refuses_missing_extra_malformed_or_unclosed_ack(self):
        import select
        import time
        for mode, payload in (("missing", b""), ("extra", b"\x01\x02"),
                              ("malformed", b"\x02"), ("unclosed", b"\x01")):
            with self.subTest(mode=mode):
                read_fd, write_fd = os.pipe()
                ack_read, ack_write = os.pipe()
                owned = {read_fd, write_fd, ack_read, ack_write}
                environment = os.environ.copy()
                environment.update({
                    "VEOVEO_UAV_READY_FD": str(write_fd),
                    "VEOVEO_UAV_ACK_FD": str(ack_read),
                    "VEOVEO_UAV_READY_DEADLINE": repr(time.monotonic() + 0.5),
                })
                child = subprocess.Popen(
                    [sys.executable, str(ENTRYPOINT)], env=environment,
                    pass_fds=(write_fd, ack_read), stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE, text=True,
                )
                try:
                    for fd in (write_fd, ack_read):
                        os.close(fd)
                        owned.remove(fd)
                    self.assertTrue(select.select([read_fd], [], [], 2.0)[0])
                    self.assertEqual(os.read(read_fd, 32), f"{child.pid}\n".encode("ascii"))
                    if payload:
                        os.write(ack_write, payload)
                    if mode != "unclosed":
                        os.close(ack_write)
                        owned.remove(ack_write)
                    stdout, stderr = child.communicate(timeout=2.0)
                    self.assertNotEqual(child.returncode, 0)
                    self.assertIn("launcher acknowledgement", stderr)
                    self.assertEqual(stdout, "")
                finally:
                    for fd in owned:
                        os.close(fd)
                    if child.poll() is None:
                        child.kill()
                        child.communicate(timeout=2.0)
