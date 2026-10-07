#!/bin/sh
# Run one owner test target with libtest's OR filters; require every named control.
set -eu
package=$1
target=$2
shift 2
controls="$*"
log=$(mktemp)
if [ "$package" = openshell-server ]; then
    set -- --features openshell-server/bundled-z3 "$target" -- "$@"
else
    set -- "$target" -- "$@"
fi
set -- cargo test --locked --release -p "$package" "$@"
case " $controls " in
    *" --ignored "*)
        python3 - <<'PYTHON'
import os
from pathlib import Path
required = (1 << 0) | (1 << 6) | (1 << 7) | (1 << 8)
fields = dict(line.split(":", 1) for line in Path("/proc/self/status").read_text().splitlines() if ":" in line)
effective = int(fields["CapEff"].strip(), 16)
if os.geteuid() != 0 or effective & required != required:
    raise SystemExit("isolated provider compiler needs root with CHOWN, SETGID, SETUID and SETPCAP for selected numeric identity/PTY controls")
print(f"isolated numeric identity/PTY controls: uid={os.geteuid()} CapEff={effective:016x} timeout=300s")
PYTHON
        set -- timeout --signal=TERM --kill-after=10s 300s "$@"
        ;;
esac
if ! "$@" > "$log" 2>&1; then
    cat "$log"
    exit 1
fi
cat "$log"
grep -Eq '^test result: ok\. [1-9][0-9]* passed;' "$log"
for control in $controls; do
    case "$control" in --ignored|--exact) continue ;; esac
    grep -Eq "^test .*${control}.* \.\.\. ok$" "$log"
done
rm "$log"
