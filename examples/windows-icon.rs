//! Generate the same ICO as the Windows build, on any development platform.
#[path = "../packaging/windows/icon.rs"]
mod icon;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let source = args
        .next()
        .ok_or("Usage: windows-icon <logo.svg> <output.ico>")?;
    let output = args
        .next()
        .ok_or("Usage: windows-icon <logo.svg> <output.ico>")?;
    if args.next().is_some() {
        return Err("Usage: windows-icon <logo.svg> <output.ico>".into());
    }
    std::fs::write(output, icon::from_svg(&std::fs::read(source)?)?)?;
    Ok(())
}
