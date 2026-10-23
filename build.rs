#[path = "packaging/windows/icon.rs"]
mod icon;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=assets/logo.svg");
    println!("cargo:rerun-if-changed=packaging/windows/icon.rs");
    println!("cargo:rerun-if-changed=packaging/windows/ryne.rc");
    if std::env::var("CARGO_CFG_TARGET_OS")? != "windows" {
        return Ok(());
    }

    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").ok_or("Missing OUT_DIR")?);
    std::fs::write(
        output.join("ryne.ico"),
        icon::from_svg(&std::fs::read("assets/logo.svg")?)?,
    )?;
    // GPUI loads icon group 1 from the executable. Fail rather than silently
    // shipping a generic icon if the Windows SDK resource compiler is missing.
    embed_resource::compile_for(
        "packaging/windows/ryne.rc",
        ["ryne"],
        embed_resource::ParamsIncludeDirs([output]),
    )
    .manifest_required()?;
    Ok(())
}
