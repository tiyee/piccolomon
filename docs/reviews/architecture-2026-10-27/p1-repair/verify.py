"""Reproduce P1 checks, isolated release queries, and native-window validation."""
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import time

here = Path(__file__).resolve().parent
root = here.parents[3]
phase = sys.argv[1] if len(sys.argv) > 1 else "all"
results = []


def run(label, command, expected=0, extra_env=None):
    started = time.monotonic()
    with (here / f"{label}.log").open("w") as output:
        result = subprocess.run(command, cwd=root, stdout=output, stderr=subprocess.STDOUT,
                                env={**os.environ, **(extra_env or {})})
    results.append({"label": label, "command": command, "expected": expected,
                    "exit_code": result.returncode, "seconds": time.monotonic() - started,
                    "environment": extra_env or {}})
    (here / f"{phase}-results.json").write_text(json.dumps(results, indent=2) + "\n")
    print(f"{label}: exit {result.returncode}, expected {expected}", flush=True)
    if result.returncode != expected:
        raise SystemExit(f"See {here / (label + '.log')}")


if phase in {"checks", "all"}:
    for label, command in [
        ("architecture", ["python3", "scripts/check-architecture.py"]),
        ("format", ["cargo", "fmt", "--check"]),
        ("diff", ["git", "diff", "--check"]),
        ("tests", ["cargo", "test", "--locked"]),
    ]:
        run(label, command)
    clippy = Path("/usr/local/opt/rust/bin/cargo-clippy")
    run("clippy", [str(clippy) if clippy.exists() else "cargo-clippy", "clippy", "--locked",
                   "--all-targets", "--features", "p1-metrics", "--", "-D", "warnings"])

if phase in {"perf", "all"}:
    (here / "environment.json").write_text(json.dumps({
        "platform": platform.platform(), "machine": platform.machine(),
        "rustc": subprocess.check_output(["rustc", "--version"], text=True).strip(),
        "baseline": "block-before.jsonl: 30 samples, 1 round, 100 warmups; pre-repair release binary",
        "final": "1000 samples, 3 rounds, 100 warmups; release with query-metrics and query allocation counters",
    }, indent=2) + "\n")
    run("perf-build", ["cargo", "build", "--locked", "--release", "-p", "ryne-syntax",
                       "--features", "query-metrics", "--example", "block-query-perf"])
    binary = root / "target/release/examples/block-query-perf"
    for label, sizes, corpus in [("block-after", "64,256,1024,10240", "all"),
                                  ("deep-list", "1024,10240", "deep-list"),
                                  ("deep-quote", "1024,10240", "deep-quote")]:
        run(label, [str(binary), sizes, "1000", "3", corpus])
        (here / f"{label}.jsonl").write_bytes((here / f"{label}.log").read_bytes())

if phase in {"smoke", "all"}:
    run("smoke-full", ["cargo", "run", "--locked", "--", "--smoke-test"])
    run("smoke-special", ["cargo", "run", "--locked", "--features", "p1-metrics", "--", "--smoke-test"],
        extra_env={"RYNE_SMOKE_P1_REPAIR_ONLY": "1"})
    run("smoke-failure", ["cargo", "run", "--locked", "--", "--smoke-test-failure"], expected=1)

if phase not in {"all", "checks", "perf", "smoke"}:
    raise SystemExit("usage: verify.py [all|checks|perf|smoke]")
