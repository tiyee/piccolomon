//! Headless feasibility experiment, deliberately outside production modules.
mod features;
mod protocol;
#[cfg(test)]
mod tests;

use features::{session, ListActions, TableActions};
use protocol::*;

fn main() -> Result<(), String> {
    let mut document = session("| A | B |\n| --- | --- |\n| 中😀 | x |\n")?;
    let host = Host::new(vec![Box::new(TableActions), Box::new(ListActions)])?;
    let contributions = host.contribute(&document)?;
    let surfaces = fixture_surfaces(&contributions);
    let context = Context::rendered(document.source.id());
    let frame = host.layout(&document, context, &surfaces, contributions)?;
    let control = frame
        .controls
        .iter()
        .find(|c| c.spec.target.part == [1, 0])
        .ok_or("no table control")?;
    let invocation = match host.activate(&document, context, &frame, control.bounds.center())? {
        Activation::Menu(mut menu) => menu.take(0)?,
        Activation::Command(command) => command,
    };
    host.execute(&mut document, context, invocation)?;
    println!("Registered render controls: {}", frame.controls.len());
    println!("After table.delete-row:\n{}", document.source.text());
    document.undo(false);
    println!("Undo restores original bytes: {}", !document.dirty());
    Ok(())
}
