//! Stable identities for bundled math fonts; no renderer or document ownership.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum MathFont {
    #[default]
    LeteSans,
    StixTwo,
}
impl MathFont {
    pub const ALL: [Self; 2] = [Self::LeteSans, Self::StixTwo];
    pub fn id(self) -> &'static str {
        match self {
            Self::LeteSans => "lete-sans-math",
            Self::StixTwo => "stix-two-math",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::LeteSans => "Lete Sans Math",
            Self::StixTwo => "STIX Two Math",
        }
    }
    pub fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|font| font.id() == id)
    }
}
