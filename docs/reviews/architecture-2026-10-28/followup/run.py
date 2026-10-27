"""Read-only audit probes; instrument copies in a temporary directory only."""
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[4]
build = subprocess.run(
    ["cargo", "build", "--locked", "-p", "ryne-services", "--message-format=json"],
    cwd=ROOT, check=True, capture_output=True, text=True,
)
libraries = {}
for line in build.stdout.splitlines():
    item = json.loads(line)
    if item.get("reason") == "compiler-artifact":
        for name in item.get("filenames", []):
            path = Path(name)
            if path.suffix == ".rlib":
                libraries[item["target"]["name"]] = path

highlight = (ROOT / "crates/ryne-services/src/highlight.rs").read_text()
highlight = highlight.split("\n#[cfg(test)]\nmod tests {")[0]
# Enable the existing parse counter in an isolated copy. No algorithms change.
highlight = highlight.replace("    #[cfg(test)]\n", "")
probe = Path(__file__).with_name("highlight_probe.rs").read_text()
actions = (ROOT / "crates/ryne-features/src/code_actions.rs").read_text()
helper = actions[actions.index("fn language_range("):actions.index("impl RenderActionProvider for CodeActions")]
helper = helper.replace(
    "parse.nodes.iter().find(|n| {",
    "parse.nodes.iter().inspect(|_| { AUDIT_VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed); }).find(|n| {",
)
assert "AUDIT_VISITS.fetch_add" in helper
control_probe = Path(__file__).with_name("control_probe.rs").read_text()
language = ROOT / "crates/ryne-services/src/language.rs"
with tempfile.TemporaryDirectory(prefix="ryne-followup-audit-") as directory:
    temporary = Path(directory)
    (temporary / "highlight.rs").write_text(highlight + "\n" + probe)
    (temporary / "code_probe.rs").write_text(
        "use ryne_extensions::EditingState;\nuse ryne_syntax::code_block;\n"
        "use std::ops::Range;\n"
        "static AUDIT_VISITS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);\n"
        + helper + "\n" + control_probe
    )
    (temporary / "main.rs").write_text(
        f"#[path = {json.dumps(str(language))}] mod language;\n"
        "mod highlight;\nmod code_probe;\n"
        "fn main() { highlight::audit(); code_probe::audit(); }\n"
    )
    binary = temporary / "probe"
    deps = libraries["ryne_extensions"].parent
    command = ["rustc", "--edition=2024", "-Awarnings", str(temporary / "main.rs"),
               "-L", f"dependency={deps}", "-o", str(binary)]
    for name in ["ryne_extensions", "ryne_syntax", "ryne_projection",
                 "editable_markdown_parser", "tree_sitter", "tree_sitter_highlight",
                 "tree_sitter_rust", "tree_sitter_python", "tree_sitter_javascript",
                 "tree_sitter_typescript", "tree_sitter_json", "tree_sitter_bash"]:
        command.extend(["--extern", f"{name}={libraries[name]}"])
    subprocess.run(command, cwd=ROOT, check=True)
    subprocess.run([str(binary)], cwd=ROOT, check=True)
