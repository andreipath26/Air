#!/usr/bin/env bash
# Linux address-space limit (not a strict RSS limit), CPU time and wall time.
set -euo pipefail
: "${AIR_VMEM_KIB:=1048576}" "${AIR_CPU_SECONDS:=30}" "${AIR_WALL_SECONDS:=60}"
[[ "$AIR_VMEM_KIB" =~ ^[1-9][0-9]*$ && "$AIR_CPU_SECONDS" =~ ^[1-9][0-9]*$ && "$AIR_WALL_SECONDS" =~ ^[1-9][0-9]*$ ]]
ulimit -v "$AIR_VMEM_KIB"
ulimit -t "$AIR_CPU_SECONDS"
exec timeout --kill-after=2s "${AIR_WALL_SECONDS}s" "$@"
