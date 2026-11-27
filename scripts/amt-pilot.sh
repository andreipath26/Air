#!/usr/bin/env bash
set -euo pipefail
if [[ $# != 2 ]]; then echo "usage: $0 SPEC.json NEW_REPORT.json" >&2; exit 2; fi
: "${AIR_MODEL_PORT:?Start local llama-server and set AIR_MODEL_PORT}"
cargo build --release -p air-cli
/usr/bin/time -v target/release/air-cli amt-convert "$1" "$2"
echo "Teacher server resource consumption is not included in the client process measurement."
