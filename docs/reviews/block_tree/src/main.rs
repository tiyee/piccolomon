use std::{env, fs, path::Path};

use ryne_block_tree_probe::Document;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = env::args_os().skip(1).collect::<Vec<_>>();
    let [input, output] = args.as_slice() else {
        return Err("usage: ryne-block-tree-probe <input.md> <new-output.md>".into());
    };
    let source = fs::read_to_string(input)?;
    Document::open(&source).save_new(Path::new(output))?;
    Ok(())
}
