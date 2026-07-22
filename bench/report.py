#!/usr/bin/env python3
"""Aggregate bench/results/*.json into a markdown report with
median + spread (min–max) per metric per stack."""

import json
import statistics
from common import RESULTS_DIR

METRICS = [
    ("boot_s", "Cold boot (s)", 1),
    ("snapshot_create_s", "Snapshot create, live w/ RAM (s)", 1),
    ("snapshot_restore_s", "Snapshot restore / revert (s)", 1),
    ("resume_from_disk_s", "Instant resume from disk (s)", 1),
    ("storage_overhead_bytes", "Snapshot storage overhead (MiB)", 1 / (1024 * 1024)),
]


def fmt(vals, scale):
    vals = [v * scale for v in vals]
    med = statistics.median(vals)
    return f"{med:.3f} (min {min(vals):.3f} / max {max(vals):.3f}, n={len(vals)})"


def main():
    files = sorted(RESULTS_DIR.glob("*.json"))
    if not files:
        raise SystemExit("no results in bench/results/; run the benchmarks first")

    print("# VMForge wedge-claim benchmark results\n")
    machine = None
    rows = []
    for f in files:
        data = json.loads(f.read_text())
        machine = data.get("machine") or machine
        if data.get("status") == "blocked":
            rows.append((data["stack"], None, data.get("reason", "blocked")))
            continue
        iters = data["iterations"]
        cells = []
        for key, _, scale in METRICS:
            vals = [it[key] for it in iters if key in it]
            cells.append(fmt(vals, scale) if vals else "—")
        rows.append((data["stack"], cells, None))

    header = "| Stack | " + " | ".join(label for _, label, _ in METRICS) + " |"
    print(header)
    print("|" + "---|" * (len(METRICS) + 1))
    for stack, cells, blocked in rows:
        if blocked:
            print(f"| {stack} | " + f"BLOCKED: {blocked} |" * 1 + " — |" * (len(METRICS) - 1))
        else:
            print(f"| {stack} | " + " | ".join(cells) + " |")

    if machine:
        print("\n## Machine\n")
        for k, v in machine.items():
            print(f"- **{k}**: {v}")


if __name__ == "__main__":
    main()
