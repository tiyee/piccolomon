"""Compile current libraries and instrument temporary copies; no production edits."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[4]
HERE = Path(__file__).resolve().parent
build = subprocess.run(
    ["cargo", "build", "--locked", "-p", "ryne-session", "-p", "ryne-features",
     "-p", "ryne-services", "--message-format=json"],
    cwd=ROOT, check=True, capture_output=True, text=True,
)
libraries = {}
for line in build.stdout.splitlines():
    item = json.loads(line)
    if item.get("reason") == "compiler-artifact":
        for filename in item.get("filenames", []):
            path = Path(filename)
            if path.suffix == ".rlib":
                libraries[item["target"]["name"]] = path

with tempfile.TemporaryDirectory(prefix="ryne-comprehensive-review-") as directory:
    temp = Path(directory)
    shutil.copytree(ROOT / "crates/ryne-features/src", temp / "features")
    presentation = temp / "features/presentation.rs"
    source = presentation.read_text()
    needle = 'insertions.retain(|(at, _, value)| *value == "\\t" || !range.contains(at));'
    assert source.count(needle) == 1
    source = source.replace(needle, '''insertions.retain(|(at, _, value)| {
        AUDIT_VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        *value == "\\t" || !range.contains(at)
    });''')
    source += '''
pub static AUDIT_VISITS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
'''
    presentation.write_text(source)
    deps = libraries["ryne_extensions"].parent
    instrumented = temp / "libryne_features_audit.rlib"
    command = ["rustc", "--edition=2024", "--crate-name=ryne_features", "--crate-type=rlib",
               str(temp / "features/lib.rs"), "-L", f"dependency={deps}", "-o", str(instrumented)]
    for name in ["ryne_core", "ryne_syntax", "ryne_projection", "ryne_extensions",
                 "ryne_parser", "unicode_segmentation"]:
        command.extend(["--extern", f"{name}={libraries[name]}"])
    subprocess.run(command, cwd=ROOT, check=True)
    libraries["ryne_features"] = instrumented
    binary = temp / "probe"
    command = ["rustc", "--edition=2024", str(HERE / "probe.rs"),
               "-L", f"dependency={deps}", "-o", str(binary)]
    for name in ["ryne_session", "ryne_features", "ryne_services", "ryne_extensions", "ryne_projection",
                 "ryne_syntax", "ryne_parser"]:
        command.extend(["--extern", f"{name}={libraries[name]}"])
    subprocess.run(command, cwd=ROOT, check=True)
    subprocess.run([str(binary)], cwd=ROOT, check=True)
