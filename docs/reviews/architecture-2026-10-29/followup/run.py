"""Build an isolated probe against the current libraries; touch only temporary fixtures."""
import json
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[4]
build = subprocess.run(
    ["cargo", "build", "--locked", "-p", "ryne-session", "-p", "ryne-features",
     "-p", "ryne-services", "--message-format=json"],
    cwd=root, check=True, capture_output=True, text=True,
)
libraries = {}
for line in build.stdout.splitlines():
    item = json.loads(line)
    if item.get("reason") == "compiler-artifact":
        for name in item.get("filenames", []):
            path = Path(name)
            if path.suffix == ".rlib":
                libraries[item["target"]["name"]] = path
with tempfile.TemporaryDirectory(prefix="ryne-review-20261029-") as directory:
    binary = Path(directory) / "probe"
    command = ["rustc", "--edition=2024", str(Path(__file__).with_name("probe.rs")),
               "-L", f"dependency={libraries['ryne_extensions'].parent}", "-o", str(binary)]
    for name in ["ryne_session", "ryne_features", "ryne_services", "ryne_extensions",
                 "ryne_syntax"]:
        command.extend(["--extern", f"{name}={libraries[name]}"])
    subprocess.run(command, cwd=root, check=True)
    subprocess.run([str(binary), directory], cwd=root, check=True)
