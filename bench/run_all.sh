#!/usr/bin/env bash
# Run the full wedge-claim benchmark suite end to end.
# Usage: run_all.sh [iterations]   (default 5)
set -euo pipefail
cd "$(dirname "$0")"
ITER="${1:-5}"

./fetch_assets.sh
python3 bench_qemu.py --mode proxy --iterations "$ITER"
python3 bench_qemu.py --mode raw --iterations "$ITER"
python3 bench_firecracker.py --iterations "$ITER"
./bench_vbox.sh "$ITER"
python3 report.py | tee results/report.md
