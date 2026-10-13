use std::{ops::Range, sync::Arc};
pub const OBJECT: &str = "\u{fffc}";
#[derive(Debug)]
pub struct Formula {
    pub path: String,
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
    pub svg: Vec<u8>,
}
#[derive(Debug)]
pub struct Diagram {
    pub pixels: Arc<[u8]>,
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub width: f32,
    pub height: f32,
    pub bytes: usize,
}
pub type DiagramOutcome = Result<Arc<Diagram>, String>;
pub type DiagramViewInput = (Range<usize>, DiagramOutcome);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Class {
    Comment,
    Keyword,
    String,
    Number,
    Function,
    Type,
    Constant,
    Property,
    Variable,
    Operator,
    Punctuation,
}

impl Class {
    // Colors for the editor's light background. Keep token styling color-only so
    // highlighting cannot change font metrics or caret geometry.
    pub fn color(self) -> u32 {
        match self {
            Self::Comment => 0x6a737d,
            Self::Keyword => 0xa626a4,
            Self::String => 0x387a35,
            Self::Number | Self::Constant => 0x986801,
            Self::Function => 0x286a9c,
            Self::Type => 0x9b5424,
            Self::Property => 0x9e3b46,
            Self::Variable => 0x383e49,
            Self::Operator | Self::Punctuation => 0x59636e,
        }
    }
}
