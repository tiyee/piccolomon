//! Standalone backend observations; not Ryne integration or GUI validation.
use pulldown_cmark::{Event, Options, Parser, Tag};

fn main() -> Result<(), String> {
    let cases = [
        ("basic", "a[^x] b[^x]\n\n[^x]: note\n"),
        ("undefined", "a[^missing]\n"),
        ("unused", "[^x]: unused\n"),
        ("duplicate", "a[^x]\n\n[^x]: first\n\n[^x]: second\n"),
        ("case", "a[^X] b[^x]\n\n[^x]: note\n"),
        ("unicode_case", "a[^STRASSE]\n\n[^straße]: note\n"),
        ("spaces", "a[^a  b]\n\n[^a b]: note\n"),
        ("escape", "a[^a\\*b]\n\n[^a\\*b]: note\n"),
        ("multiline", "a[^x]\n\n[^x]: first\n\n    second\n\n    - item\n\n    ```rs\n    let x = 1;\n    ```\n"),
        ("empty", "a[^x]\n\n[^x]:\n"),
        ("nested_quote", "a[^x]\n\n> [^x]: quote\n"),
        ("nested_list", "a[^x]\n\n- [^x]: list\n"),
        ("recursive", "a[^x]\n\n[^x]: b[^y]\n\n[^y]: c[^x]\n"),
        ("table_ref", "| a |\n| --- |\n| [^x] |\n\n[^x]: note\n"),
        ("literal", "`[^x]` \\[^x]\n\n```\n[^x]\n```\n\n<div>\n[^x]\n</div>\n\n[^x]: note\n"),
        ("crlf", "a[^x]\r\n\r\n[^x]: first\r\n\r\n    second\r\n"),
        ("cr", "a[^x]\r\r[^x]: first\r\r    second\r"),
        ("lazy", "a[^x]\n\n[^x]: first\ncontinued\n\noutside\n"),
        ("definition_order", "a[^y] b[^x]\n\n[^x]: X\n\n[^y]: Y\n"),
        ("link_collision", "a[^x] b[x]\n\n[^x]: note\n[x]: /url\n"),
        ("table_pipe", "| a |\n| --- |\n| [^a\\|b] |\n\n[^a|b]: note\n"),
        ("inline_math", "$[^x]$\n\n[^x]: note\n"),
    ];
    let base = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    println!("Backend: pulldown-cmark 0.13.6; byte ranges are half-open UTF-8.");
    println!("Options: base GFM subset + ENABLE_FOOTNOTES + ENABLE_MATH; no OLD_FOOTNOTES.");
    for (name, source) in cases {
        let baseline = Parser::new_ext(source, base)
            .filter(|event| {
                matches!(
                    event,
                    Event::FootnoteReference(_) | Event::Start(Tag::FootnoteDefinition(_))
                )
            })
            .count();
        if baseline != 0 {
            return Err(format!(
                "{name}: default baseline unexpectedly emits footnotes"
            ));
        }
        println!("\nCASE {name}\nSOURCE {source:?}");
        for (event, range) in Parser::new_ext(source, base).into_offset_iter() {
            if matches!(&event, Event::Start(Tag::Link { .. })) {
                println!("BASE_LINK {range:?} {event:?}");
            }
        }
        let mut references = 0;
        let mut definitions = 0;
        for (event, range) in Parser::new_ext(
            source,
            base | Options::ENABLE_FOOTNOTES | Options::ENABLE_MATH,
        )
        .into_offset_iter()
        {
            let raw = source
                .get(range.clone())
                .ok_or_else(|| format!("{name}: invalid range {range:?}"))?;
            match &event {
                Event::FootnoteReference(_) => references += 1,
                Event::Start(Tag::FootnoteDefinition(_)) => definitions += 1,
                _ => {}
            }
            println!("{range:?} {event:?} RAW {raw:?}");
        }
        println!(
            "COUNTS refs={references} definitions={definitions} baseline_footnotes={baseline}"
        );
        let expected = match name {
            "basic" | "case" => (2, 1),
            "undefined" => (0, 0),
            "unused" | "literal" | "inline_math" => (0, 1),
            "duplicate" => (1, 2),
            "recursive" => (3, 2),
            "definition_order" => (2, 2),
            _ => (1, 1),
        };
        if (references, definitions) != expected {
            return Err(format!(
                "{name}: counts {:?} differ from {expected:?}",
                (references, definitions)
            ));
        }
    }
    println!("\nPASS: {} cases match expected reference/definition counts; all event ranges are valid UTF-8 slices and baseline footnotes remain disabled.", cases.len());
    Ok(())
}
