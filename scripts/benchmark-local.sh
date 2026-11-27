#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
count="${1:-10000}"
out="$(mktemp -d)"
trap 'rm -rf "$out"' EXIT
cargo build --release -p air-bench -p air-cli
echo "Hardware: $(uname -sm)" >&2
if command -v lscpu >/dev/null; then lscpu >&2; fi
if [ -x /usr/bin/time ]; then
  /usr/bin/time -v target/release/air-bench retrieval "$count" "$out/bench.db"
else
  target/release/air-bench retrieval "$count" "$out/bench.db"
fi
