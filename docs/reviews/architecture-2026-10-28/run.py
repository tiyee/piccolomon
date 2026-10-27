"""Compile an isolated audit probe; do not add Cargo targets or change production code."""
import json
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
build = subprocess.run(
    ["cargo", "build", "--locked", "-p", "ryne-session", "-p", "ryne-features",
     "-p", "ryne-services", "--message-format=json"],
    cwd=root, check=True, capture_output=True, text=True,
)
libraries = {}
deps = None
for line in build.stdout.splitlines():
    item = json.loads(line)
    if item.get("reason") != "compiler-artifact":
        continue
    for filename in item.get("filenames", []):
        path = Path(filename)
        if path.suffix == ".rlib":
            libraries[item["target"]["name"]] = path
            if path.parent.name == "deps":
                deps = path.parent
with tempfile.TemporaryDirectory(prefix="ryne-audit-20261028-") as temporary:
    binary = Path(temporary) / "probe"
    command = ["rustc", "--edition=2024", str(Path(__file__).with_name("probe.rs")),
               "-L", f"dependency={deps}", "-o", str(binary)]
    for name in ["ryne_session", "ryne_features", "ryne_services", "ryne_extensions",
                 "ryne_syntax", "editable_markdown_parser"]:
        command.extend(["--extern", f"{name}={libraries[name]}"])
    subprocess.run(command, cwd=root, check=True)
    subprocess.run([str(binary)], cwd=root, check=True)
