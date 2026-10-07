"""Private Candidate cleanup; retained Linux pidfds bind every actual signal."""
import os
import select
import signal
import sys
import time


def identity(pid):
    with open(f"/proc/{pid}/stat", encoding="ascii") as source:
        fields = source.read().rsplit(") ", 1)[1].split()
    return int(fields[2]), int(fields[19]), fields[0]


def bound_identity(fd, pid):
    def bound_pid():
        with open(f"/proc/self/fdinfo/{fd}", encoding="ascii") as source:
            entries = dict(line.split(":", 1) for line in source if ":" in line)
        return int(entries["Pid"].strip())
    if bound_pid() != pid:
        raise RuntimeError("candidate pidfd identity no longer names the admitted process")
    value = identity(pid)
    # A reaped original can leave the same numeric PID in /proc for a new
    # process, but its retained pidfd then reports Pid -1. Check BOTH sides.
    if bound_pid() != pid:
        raise RuntimeError("candidate pidfd identity changed during process inspection")
    return value


def members(group):
    found = []
    for entry in os.scandir("/proc"):
        if entry.name.isdecimal():
            pid = int(entry.name)
            try:
                observed_group, _, state = identity(pid)
            except FileNotFoundError:
                continue
            if observed_group == group and state != "Z":
                found.append(pid)
    return found


def exited(fd):
    poll = select.poll()
    poll.register(fd, select.POLLIN)
    return bool(poll.poll(0))


def stop(group, start_ticks, timeout):
    if not hasattr(os, "pidfd_open") or not hasattr(signal, "pidfd_send_signal"):
        raise RuntimeError("Candidate cleanup requires Linux pidfd support")
    end = time.monotonic() + timeout
    leader = os.pidfd_open(group)
    held = []
    try:
        observed_group, ticks, _ = bound_identity(leader, group)
        if observed_group != group or ticks != start_ticks or exited(leader):
            raise RuntimeError("candidate leader identity changed; retain unresolved intent")
        while True:
            if time.monotonic() >= end:
                raise RuntimeError("candidate children did not settle within cleanup deadline")
            if exited(leader):
                raise RuntimeError("candidate leader exited before member cleanup; retain unresolved intent")
            children = [pid for pid in members(group) if pid != group]
            if not children:
                break
            for pid in children:
                # Open first, then verify membership. A reused numeric PID must
                # satisfy the original group while its retained handle is live.
                try:
                    fd = os.pidfd_open(pid)
                except ProcessLookupError:
                    continue
                held.append(fd)
                try:
                    child_group, _, _ = bound_identity(fd, pid)
                except FileNotFoundError:
                    if exited(fd):
                        continue
                    raise
                if child_group != group or exited(leader):
                    raise RuntimeError("candidate child identity changed; refuse signalling")
                try:
                    signal.pidfd_send_signal(fd, signal.SIGTERM)
                except ProcessLookupError:
                    pass
            time.sleep(min(0.05, max(0, end - time.monotonic())))
        if exited(leader):
            raise RuntimeError("candidate leader exited before terminal signal; retain unresolved intent")
        signal.pidfd_send_signal(leader, signal.SIGTERM)
        while not exited(leader) or any(not exited(fd) for fd in held):
            if time.monotonic() >= end:
                raise RuntimeError("candidate processes did not settle within cleanup deadline")
            time.sleep(min(0.05, max(0, end - time.monotonic())))
        if any(pid != group for pid in members(group)):
            raise RuntimeError("candidate group still has live members; retain unresolved intent")
    finally:
        for fd in held:
            os.close(fd)
        os.close(leader)


if __name__ == "__main__":
    stop(int(sys.argv[1]), int(sys.argv[2]), float(sys.argv[3]))
