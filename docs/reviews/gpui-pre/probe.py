#!/usr/bin/env python3
"""Reproduce the GPUI compile probe in a fresh directory, without editing Ryne."""

import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prepare-only", action="store_true")
    parser.add_argument("--clippy", action="store_true")
    args = parser.parse_args()
    evidence = Path(__file__).resolve().parent
    repository = evidence.parents[2]
    checks = json.loads((evidence / "checks.json").read_text())
    lock = (evidence / "candidate.Cargo.lock").read_bytes()
    if hashlib.sha256(lock).hexdigest() != checks["candidate_lock_sha256"]:
        raise SystemExit("Candidate lock checksum mismatch")

    scratch = Path(tempfile.mkdtemp(prefix="ryne-gpui-pre-reproduce-"))
    workspace = scratch / "workspace"
    workspace.mkdir()
    archive = subprocess.check_output(
        ["git", "archive", "--format=tar", checks["baseline"]], cwd=repository
    )
    with tarfile.open(fileobj=io.BytesIO(archive)) as source:
        source.extractall(workspace, filter="data")
    patch = str(evidence / "compile-probe.patch")
    subprocess.run(["git", "apply", "--check", patch], cwd=workspace, check=True)
    subprocess.run(["git", "apply", patch], cwd=workspace, check=True)
    (workspace / "Cargo.lock").write_bytes(lock)

    cargo_home = scratch / "cargo-home"
    cargo_home.mkdir()
    previous_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    registry = previous_home / "registry"
    if registry.is_dir():
        (cargo_home / "registry").symlink_to(registry.resolve(), target_is_directory=True)
    env = os.environ.copy()
    env["CARGO_HOME"] = str(cargo_home)
    env["CARGO_TARGET_DIR"] = str(scratch / "target")
    print(f"Probe workspace: {workspace}", flush=True)
    print(f"Isolated Cargo home: {cargo_home}", flush=True)
    if args.prepare_only:
        return 0

    commands = [["cargo", "check", "--locked", "--workspace", "--all-targets"]]
    if args.clippy:
        commands.append([
            "cargo", "clippy", "--locked", "--workspace", "--all-targets",
            "--", "-D", "warnings",
        ])
    for command in commands:
        log = scratch / f"{command[1]}.log"
        with log.open("w") as output:
            result = subprocess.run(
                command, cwd=workspace, env=env,
                stdout=output, stderr=subprocess.STDOUT,
            )
        print(f"{' '.join(command)}: exit {result.returncode}; log: {log}", flush=True)
        if result.returncode:
            print(log.read_text()[-8000:])
            return result.returncode
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
