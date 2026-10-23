use editable_markdown_parser::{Error, Source};
pub struct Utf16Index {
    source: Source,
    checkpoints: Vec<(usize, usize)>,
}
impl Utf16Index {
    pub fn new(source: Source) -> Self {
        let mut checkpoints = vec![(0, 0)];
        let mut units = 0;
        let mut prior = 0;
        for (at, ch) in source.text().char_indices() {
            if at - prior >= 4096 {
                checkpoints.push((at, units));
                prior = at;
            }
            units += ch.len_utf16();
        }
        checkpoints.push((source.text().len(), units));
        Self {
            source,
            checkpoints,
        }
    }
    pub fn byte_to_utf16(&self, at: usize) -> Result<usize, Error> {
        if !self.source.text().is_char_boundary(at) {
            return Err(Error::InvalidRange);
        }
        let i = self.checkpoints.partition_point(|&(b, _)| b <= at) - 1;
        let (byte, units) = self.checkpoints[i];
        Ok(units + self.source.text()[byte..at].encode_utf16().count())
    }
    pub fn utf16_to_byte(&self, at: usize) -> Result<usize, Error> {
        let i = self
            .checkpoints
            .partition_point(|&(_, u)| u <= at)
            .saturating_sub(1);
        let (byte, units) = self.checkpoints[i];
        editable_markdown_parser::utf16_to_utf8(&self.source.text()[byte..], at - units)
            .map(|p| byte + p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoints_match_platform_units_and_reject_split_surrogates() {
        for text in [
            "a😀中e\u{301}\r\n".to_owned(),
            "a😀中e\u{301}\r\n".repeat(1200),
        ] {
            let index = Utf16Index::new(Source::new(&text));
            for at in (0..=text.len()).filter(|p| *p < 32 || p % 127 == 0 || *p == text.len()) {
                let expected = text
                    .get(..at)
                    .map(|s| s.encode_utf16().count())
                    .ok_or(Error::InvalidRange);
                assert_eq!(index.byte_to_utf16(at), expected);
            }
            let units = text.encode_utf16().count();
            for at in (0..=units + 1).filter(|p| *p < 32 || p % 127 == 0 || *p >= units) {
                assert_eq!(
                    index.utf16_to_byte(at),
                    editable_markdown_parser::utf16_to_utf8(&text, at)
                );
            }
        }
    }
}
