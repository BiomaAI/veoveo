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
# Save filters before adding Cargo switches.
if ! cargo test --locked --release -p "$package" "$@" > "$log" 2>&1; then
    cat "$log"
    exit 1
fi
cat "$log"
grep -Eq '^test result: ok\. [1-9][0-9]* passed;' "$log"
for control in $controls; do
    grep -Eq "^test .*${control}.* \.\.\. ok$" "$log"
done
rm "$log"
