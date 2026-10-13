#!/usr/bin/env python3
"""Summarize RYNE_PROFILE records from a smoke log as JSON lines."""
import json
from pathlib import Path
import sys

samples = {}
for line in Path(sys.argv[1]).read_text().splitlines():
    if line.startswith("RYNE_PROFILE "):
        record = json.loads(line.removeprefix("RYNE_PROFILE "))
        samples.setdefault(record["metric"], []).append(record["elapsed_ms"])
for metric, values in sorted(samples.items()):
    values.sort()
    count = len(values)
    print(json.dumps({
        "metric": metric,
        "samples": count,
        "p50_ms": values[count // 2],
        "p95_ms": values[min(count - 1, count * 95 // 100)],
        "p99_ms": values[min(count - 1, count * 99 // 100)],
    }))
