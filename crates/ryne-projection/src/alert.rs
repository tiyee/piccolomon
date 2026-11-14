//! Shared alert presentation tokens for native UI and printing.
use ryne_syntax::alert::Kind;
pub fn icon(kind: Kind) -> &'static str {
    match kind {
        Kind::Note => "alert_note",
        Kind::Tip => "alert_tip",
        Kind::Important => "alert_important",
        Kind::Warning => "alert_warning",
        Kind::Caution => "alert_caution",
    }
}
pub fn color(kind: Kind, dark: bool) -> u32 {
    match (kind, dark) {
        (Kind::Note, false) => 0x0969da,
        (Kind::Note, true) => 0x2f81f7,
        (Kind::Tip, false) => 0x1a7f37,
        (Kind::Tip, true) => 0x3fb950,
        (Kind::Important, false) => 0x8250df,
        (Kind::Important, true) => 0xa371f7,
        (Kind::Warning, false) => 0x9a6700,
        (Kind::Warning, true) => 0xd29922,
        (Kind::Caution, false) => 0xd1242f,
        (Kind::Caution, true) => 0xf85149,
    }
}
pub fn background(kind: Kind, paper: u32, dark: bool) -> u32 {
    let accent = color(kind, dark);
    let weight = if dark { 10 } else { 6 };
    [16, 8, 0].into_iter().fold(0, |result, shift| {
        let a = (accent >> shift) & 255;
        let p = (paper >> shift) & 255;
        result | (((a * weight + p * (100 - weight) + 50) / 100) << shift)
    })
}

/// Self-contained status SVGs, shared by the editor asset bridge and PDF output.
pub fn svg(kind: Kind) -> &'static str {
    match kind {
        Kind::Note => include_str!("../../../assets/icons/alert_note.svg"),
        Kind::Tip => include_str!("../../../assets/icons/alert_tip.svg"),
        Kind::Important => include_str!("../../../assets/icons/alert_important.svg"),
        Kind::Warning => include_str!("../../../assets/icons/alert_warning.svg"),
        Kind::Caution => include_str!("../../../assets/icons/alert_caution.svg"),
    }
}
