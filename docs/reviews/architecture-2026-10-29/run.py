"""Audit current source via isolated probes; production files remain unchanged."""
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
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

actions = (ROOT / "crates/ryne-features/src/list_actions.rs").read_text()
actions = actions[:actions.index("impl RenderActionProvider for ListActions")]
actions = actions.replace("use crate::commands::Planner;\n", "")
needle = ".any(|block| block.range.contains(&line.start))"
indexed = needle not in actions
if indexed:
    needle = "block.range.end <= line.start"
    assert actions.count(needle) == 1
    actions = actions.replace(needle, "{ VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed); block.range.end <= line.start }")
else:
    assert actions.count(needle) == 1
    actions = actions.replace(needle, ".any(|block| { VISITS.fetch_add(1, std::sync::atomic::Ordering::Relaxed); block.range.contains(&line.start) })")
actions += f"\nconst INDEXED: bool = {str(indexed).lower()};\n"
actions += '''
static VISITS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
pub fn audit() {
    use std::sync::atomic::Ordering;
    for count in [100usize, 200, 400, 800] {
        let raw = "- item\\n  continuation\\n\\n".repeat(count)
            + &"```rust\\nlet x = 1;\\n```\\n\\n".repeat(count);
        let state = EditingState::new(ryne_parser::Source::new(&raw));
        let context = Extents::new(&state);
        let items = list::items(&state.parse);
        assert_eq!(items.len(), count);
        assert_eq!(context.code.len(), count);
        VISITS.store(0, Ordering::Relaxed);
        for item in &items { std::hint::black_box(context.get(item)); }
        let visits = VISITS.load(Ordering::Relaxed);
        if INDEXED {
            assert!(visits <= 2 * count * (count.ilog2() as usize + 2));
        } else {
            assert!(visits >= count * count);
        }
        println!("list_extent indexed={INDEXED} items={count} code_blocks={count} code_checks={visits}");
    }
}
'''
with tempfile.TemporaryDirectory(prefix="ryne-review-20261029-") as directory:
    temporary = Path(directory)
    (temporary / "list_probe.rs").write_text(actions)
    (temporary / "main.rs").write_text(Path(__file__).with_name("probe.rs").read_text())
    binary = temporary / "probe"
    command = ["rustc", "--edition=2024", "-Awarnings", str(temporary / "main.rs"),
               "-L", f"dependency={libraries['ryne_extensions'].parent}", "-o", str(binary)]
    for name in ["ryne_session", "ryne_features", "ryne_services", "ryne_extensions",
                 "ryne_syntax", "ryne_parser"]:
        command.extend(["--extern", f"{name}={libraries[name]}"])
    subprocess.run(command, cwd=ROOT, check=True)
    subprocess.run([str(binary), str(temporary)], cwd=ROOT, check=True)
