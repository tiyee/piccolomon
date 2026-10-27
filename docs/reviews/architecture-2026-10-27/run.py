"""Run the isolated review probes without adding Cargo targets or editing production code."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

root = Path(__file__).resolve().parents[3]
probe = sys.argv[1] if len(sys.argv) > 1 else "repro"
if probe not in {"repro", "scaling", "third-cache"}:
    raise SystemExit("usage: python3 run.py [repro|scaling|third-cache]")
subprocess.run(
    ["cargo", "build", "--locked", "-p", "ryne-session", "-p", "ryne-features", "-p", "ryne-services"],
    cwd=root,
    check=True,
)
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=root
))
deps = Path(metadata["target_directory"]) / "debug" / "deps"
with tempfile.TemporaryDirectory(prefix="ryne-review-") as temporary:
    binary = Path(temporary) / probe
    command = [
        "rustc", "--edition=2024", str(Path(__file__).with_name(f"{probe}.rs")),
        "-L", f"dependency={deps}", "-o", str(binary),
    ]
    for name in ["session", "features", "services", "extensions", "syntax", "projection"]:
        library = max(deps.glob(f"libryne_{name}-*.rlib"), key=lambda p: p.stat().st_mtime_ns)
        command.extend(["--extern", f"ryne_{name}={library}"])
    subprocess.run(command, cwd=root, check=True)
    subprocess.run([str(binary)], cwd=root, check=True)
