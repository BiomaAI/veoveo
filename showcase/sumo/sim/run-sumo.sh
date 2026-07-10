#!/bin/sh
set -eu

work_dir="$(mktemp -d /tmp/veoveo-sumo.XXXXXX)"
cp -a /lust/scenario/. "$work_dir/"
cd "$work_dir"
exec sumo "$@"
