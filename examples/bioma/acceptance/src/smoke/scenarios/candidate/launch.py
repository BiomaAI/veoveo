"""Private launch receipt from the exact process which execs the candidate."""
import json
import os
import signal
import sys


def prerequisite():
    if not hasattr(os, "pidfd_open") or not hasattr(signal, "pidfd_send_signal"):
        raise RuntimeError("Candidate requires Python/Linux pidfd cleanup before launch")
    handle = os.pidfd_open(os.getpid())
    try:
        signal.pidfd_send_signal(handle, 0)
    finally:
        os.close(handle)


def launch(receipt, invocation, executable, arguments):
    prerequisite()
    os.setsid()
    with open("/proc/self/stat", encoding="ascii") as source:
        fields = source.read().rsplit(") ", 1)[1].split()
    value = {"format": "veoveo.ai/candidate-launch/v1", "invocation": invocation,
             "processGroup": os.getpid(), "startTicks": int(fields[19])}
    temporary = receipt + ".pending"
    fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as output:
        json.dump(value, output)
        output.flush()
        os.fsync(output.fileno())
    os.replace(temporary, receipt)
    os.execv(executable, [executable, *arguments])


if __name__ == "__main__":
    if len(sys.argv) == 2 and sys.argv[1] == "--prerequisite":
        prerequisite()
    else:
        launch(sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:])
