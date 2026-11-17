#!/bin/sh
set -eu
script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
project_dir=$(CDPATH= cd -- "$script_dir/../.." && pwd)
cd "$project_dir"
cargo +1.99.1 build -p threadkeeper
python3 desktop-rune/scripts/package.py
open "$project_dir/desktop-rune/dist/Threadkeeper.app"
