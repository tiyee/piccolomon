#!/usr/bin/env python3
"""Summarize RYNE_PROFILE records from a smoke log as JSON lines."""
import json
from pathlib import Path
import sys

samples = {}
counters = {}
memory = None
memory_peaks = {}
for line in Path(sys.argv[1]).read_text().splitlines():
    if line.startswith("RYNE_PROFILE "):
        record = json.loads(line.removeprefix("RYNE_PROFILE "))
        if "elapsed_ms" in record:
            samples.setdefault(record["metric"], []).append(record["elapsed_ms"])
        elif "count" in record:
            counters[record["metric"]] = record["count"]
        elif record["metric"] == "owned_memory_sample":
            memory = record
            for key in ("history_reserved_bytes", "published_controls_reserved_bytes", "published_controls", "published_actions"):
                memory_peaks[key] = max(memory_peaks.get(key, 0), record[key])
for metric, count in sorted(counters.items()):
    print(json.dumps({"metric": metric, "count": count}))
if memory is not None:
    print(json.dumps(memory))
    print(json.dumps({"metric": "owned_memory_peaks", **memory_peaks}))
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
