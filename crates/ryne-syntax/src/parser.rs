//! Bare-CR compatibility is confined to a same-byte-width semantic view.
use editable_markdown_parser::{ParseSnapshot, Source};
pub fn normalized(parse: &ParseSnapshot) -> Option<ParseSnapshot> {
    let text = parse.source().text();
    text.bytes()
        .enumerate()
        .any(|(i, b)| b == b'\r' && text.as_bytes().get(i + 1) != Some(&b'\n'))
        .then(|| {
            let text = text
                .char_indices()
                .map(|(i, ch)| {
                    if ch == '\r' && text.as_bytes().get(i + 1) != Some(&b'\n') {
                        '\n'
                    } else {
                        ch
                    }
                })
                .collect::<String>();
            ParseSnapshot::with_dialect_and_math(
                Source::new(&text),
                parse.dialect,
                parse.math_enabled,
            )
        })
}
