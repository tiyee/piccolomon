//! Code highlighting is a disposable view of Markdown source, never editable data.
pub use ryne_projection::Class;
use ryne_syntax::code_block::CodeBlock;
use std::{collections::HashMap, ops::Range};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

const CAPTURES: &[(&str, Class)] = &[
    ("comment", Class::Comment),
    ("keyword", Class::Keyword),
    ("string", Class::String),
    ("escape", Class::String),
    ("number", Class::Number),
    ("function", Class::Function),
    ("constructor", Class::Function),
    ("type", Class::Type),
    ("constant", Class::Constant),
    ("boolean", Class::Constant),
    ("property", Class::Property),
    ("variable", Class::Variable),
    ("operator", Class::Operator),
    ("punctuation", Class::Punctuation),
    ("tag", Class::Type),
    ("attribute", Class::Property),
    ("embedded", Class::Punctuation),
];

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
enum Language {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Json,
    Bash,
}

impl Language {
    fn from_info(info: &str) -> Option<Self> {
        match info
            .split_whitespace()
            .next()?
            .to_ascii_lowercase()
            .as_str()
        {
            "rust" | "rs" => Some(Self::Rust),
            "python" | "py" | "python3" => Some(Self::Python),
            "javascript" | "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "typescript" | "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "json" | "jsonc" => Some(Self::Json),
            "bash" | "sh" | "shell" => Some(Self::Bash),
            _ => None,
        }
    }

    fn configuration(self) -> Result<HighlightConfiguration, String> {
        let js = || {
            format!(
                "{}\n{}",
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY
            )
        };
        let ts = || {
            format!(
                "{}\n{}",
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
                tree_sitter_javascript::HIGHLIGHT_QUERY
            )
        };
        let (grammar, query, locals) = match self {
            Self::Rust => (
                tree_sitter_rust::LANGUAGE,
                tree_sitter_rust::HIGHLIGHTS_QUERY.to_owned(),
                "",
            ),
            Self::Python => (
                tree_sitter_python::LANGUAGE,
                tree_sitter_python::HIGHLIGHTS_QUERY.to_owned(),
                "",
            ),
            Self::JavaScript => (
                tree_sitter_javascript::LANGUAGE,
                js(),
                tree_sitter_javascript::LOCALS_QUERY,
            ),
            Self::TypeScript => (
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
                ts(),
                tree_sitter_typescript::LOCALS_QUERY,
            ),
            Self::Tsx => (
                tree_sitter_typescript::LANGUAGE_TSX,
                format!("{}\n{}", ts(), tree_sitter_javascript::JSX_HIGHLIGHT_QUERY),
                tree_sitter_typescript::LOCALS_QUERY,
            ),
            Self::Json => (
                tree_sitter_json::LANGUAGE,
                tree_sitter_json::HIGHLIGHTS_QUERY.to_owned(),
                "",
            ),
            Self::Bash => (
                tree_sitter_bash::LANGUAGE,
                tree_sitter_bash::HIGHLIGHT_QUERY.to_owned(),
                "",
            ),
        };
        // Embedded-language injections can be added independently later. The
        // outer Markdown parser continues to decide the code block boundaries.
        let mut config =
            HighlightConfiguration::new(grammar.into(), format!("{self:?}"), &query, "", locals)
                .map_err(|error| error.to_string())?;
        config.configure(&CAPTURES.iter().map(|(name, _)| *name).collect::<Vec<_>>());
        Ok(config)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub class: Class,
}

struct CachedBlock {
    language: Language,
    text: String,
    spans: Vec<Span>,
    used: bool,
}

const MAX_BLOCK_BYTES: usize = 256 * 1024;
const MAX_CACHE_BYTES: usize = 2 * 1024 * 1024;
const MAX_CACHE_ENTRIES: usize = 64;

#[derive(Default)]
pub struct CodeHighlighter {
    highlighter: Highlighter,
    configurations: HashMap<Language, Option<HighlightConfiguration>>,
    cache: Vec<CachedBlock>,
    #[cfg(test)]
    parses: usize,
}

impl CodeHighlighter {
    pub fn cache_entries(&self) -> usize {
        self.cache.len()
    }
    pub fn begin_frame(&mut self) {
        for block in &mut self.cache {
            block.used = false;
        }
    }

    pub fn end_frame(&mut self) {
        self.cache.retain(|block| block.used);
    }

    /// Return UTF-8 source ranges, excluding fences and container indentation.
    pub fn spans(&mut self, source: &str, block: &CodeBlock) -> Vec<Span> {
        let Some(language) = Language::from_info(&block.language) else {
            return Vec::new();
        };
        let size: usize = block.rows.iter().map(|row| row.content.len() + 1).sum();
        if size == 0 || size > MAX_BLOCK_BYTES {
            return Vec::new();
        }
        let mut code = String::with_capacity(size);
        let mut mappings = Vec::with_capacity(block.rows.len());
        for row in &block.rows {
            let start = code.len();
            code.push_str(&source[row.content.clone()]);
            mappings.push((start..code.len(), row.content.clone()));
            // Normalize only this derived buffer. Source bytes and line endings
            // remain untouched; every code row is parsed in its full context.
            if source[row.line.clone()].ends_with(['\r', '\n']) {
                code.push('\n');
            }
        }
        let spans = if let Some(cached) = self
            .cache
            .iter_mut()
            .find(|entry| entry.language == language && entry.text == code)
        {
            cached.used = true;
            cached.spans.clone()
        } else {
            let spans = self.highlight(language, &code);
            while self.cache.len() >= MAX_CACHE_ENTRIES
                || self
                    .cache
                    .iter()
                    .map(|entry| entry.text.len())
                    .sum::<usize>()
                    + code.len()
                    > MAX_CACHE_BYTES
            {
                let Some(index) = self.cache.iter().position(|entry| !entry.used) else {
                    break;
                };
                self.cache.remove(index);
            }
            if self.cache.len() < MAX_CACHE_ENTRIES
                && self
                    .cache
                    .iter()
                    .map(|entry| entry.text.len())
                    .sum::<usize>()
                    + code.len()
                    <= MAX_CACHE_BYTES
            {
                self.cache.push(CachedBlock {
                    language,
                    text: code,
                    spans: spans.clone(),
                    used: true,
                });
            }
            spans
        };
        let mut mapped = Vec::new();
        let mut row = 0;
        for span in spans {
            while row < mappings.len() && mappings[row].0.end <= span.range.start {
                row += 1;
            }
            for (code_range, source_range) in mappings[row..]
                .iter()
                .take_while(|(r, _)| r.start < span.range.end)
            {
                let start = span.range.start.max(code_range.start);
                let end = span.range.end.min(code_range.end);
                if start < end {
                    mapped.push(Span {
                        range: source_range.start + start - code_range.start
                            ..source_range.start + end - code_range.start,
                        class: span.class,
                    });
                }
            }
        }
        mapped
    }

    fn highlight(&mut self, language: Language, code: &str) -> Vec<Span> {
        let config = self.configurations.entry(language).or_insert_with(|| {
            language
                .configuration()
                .map_err(|error| {
                    eprintln!("Code highlighting unavailable for {language:?}: {error}");
                })
                .ok()
        });
        let Some(config) = config else {
            return Vec::new();
        };
        #[cfg(test)]
        {
            self.parses += 1;
        }
        let Ok(events) = self
            .highlighter
            .highlight(config, code.as_bytes(), None, None, |_| None)
        else {
            return Vec::new();
        };
        let mut stack = Vec::new();
        let mut spans = Vec::<Span>::new();
        for event in events {
            match event {
                Ok(HighlightEvent::HighlightStart(highlight)) => {
                    stack.push(CAPTURES[highlight.0].1)
                }
                Ok(HighlightEvent::HighlightEnd) => {
                    stack.pop();
                }
                Ok(HighlightEvent::Source { start, end }) => {
                    if let Some(&class) = stack.last()
                        && start < end
                    {
                        if let Some(last) = spans
                            .last_mut()
                            .filter(|span| span.class == class && span.range.end == start)
                        {
                            last.range.end = end;
                        } else {
                            spans.push(Span {
                                range: start..end,
                                class,
                            });
                        }
                    }
                }
                // Never publish partial highlights on cancellation or failure.
                Err(_) => return Vec::new(),
            }
        }
        spans
    }
}

impl ryne_extensions::SyntaxHighlighter for CodeHighlighter {
    fn begin_frame(&mut self) {
        CodeHighlighter::begin_frame(self);
    }
    fn end_frame(&mut self) {
        CodeHighlighter::end_frame(self);
    }
    fn spans(&mut self, source: &str, block: &CodeBlock) -> Vec<ryne_extensions::HighlightSpan> {
        CodeHighlighter::spans(self, source, block)
            .into_iter()
            .map(|s| ryne_extensions::HighlightSpan {
                range: s.range,
                class: s.class,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use editable_markdown_parser::{ParseSnapshot, Source};
    use ryne_syntax::code_block;

    fn spans(highlighter: &mut CodeHighlighter, raw: &str) -> Vec<Span> {
        let parse = ParseSnapshot::new(Source::new(raw));
        let blocks = code_block::blocks(&parse);
        assert_eq!(blocks.len(), 1, "{raw:?}");
        highlighter.begin_frame();
        let spans = highlighter.spans(raw, &blocks[0]);
        highlighter.end_frame();
        spans
    }

    #[test]
    fn bundled_grammars_and_aliases_highlight_real_tokens() {
        let mut highlighter = CodeHighlighter::default();
        for (language, code, token, class) in [
            ("Rust", "fn main() { let n = 42; }", "fn", Class::Keyword),
            ("py", "def main():\n    return 42", "def", Class::Keyword),
            ("js", "const n = 'hello';", "const", Class::Keyword),
            (
                "jsx",
                "const view = <main title=\"hi\" />;",
                "main",
                Class::Type,
            ),
            (
                "ts",
                "interface User { name: string }",
                "interface",
                Class::Keyword,
            ),
            (
                "tsx",
                "const view = <main title=\"hi\" />;",
                "main",
                Class::Type,
            ),
            ("json", "{\"name\": \"hi\", \"n\": 42}", "42", Class::Number),
            ("sh", "# hello\necho \"hi\"", "# hello", Class::Comment),
        ] {
            let raw = format!("```{language}\n{code}\n```\n");
            let spans = spans(&mut highlighter, &raw);
            let start = raw.find(token).unwrap();
            assert!(
                spans
                    .iter()
                    .any(|span| span.range.contains(&start) && span.class == class),
                "{language}: {spans:?}"
            );
            assert!(highlighter.configurations.values().all(Option::is_some));
        }
        assert_eq!(
            Language::from_info("  PYTHON3 extra"),
            Some(Language::Python)
        );
        assert_eq!(Language::from_info("jsonc"), Some(Language::Json));
        assert_eq!(Language::from_info("mjs"), Some(Language::JavaScript));
        assert_eq!(Language::from_info("mts"), Some(Language::TypeScript));
    }

    #[test]
    fn multiline_tokens_map_through_nested_prefixes_and_all_line_endings() {
        for newline in ["\n", "\r\n", "\r"] {
            for (opening, prefix, closing) in [
                ("```rust", "", "```"),
                ("~~~~rs", "", "~~~~"),
                ("> ```rust", "> ", "> ```"),
                ("- ```rust", "  ", "  ```"),
                ("> - ```rust", ">   ", ">   ```"),
            ] {
                let raw = format!(
                    "{opening}{newline}{prefix}/* start{newline}{prefix}**中😀** */{newline}{prefix}let s = \"中文😀\";{newline}{closing}{newline}"
                );
                let mut highlighter = CodeHighlighter::default();
                let spans = spans(&mut highlighter, &raw);
                for (token, class) in [
                    ("**中😀**", Class::Comment),
                    ("let", Class::Keyword),
                    ("中文😀", Class::String),
                ] {
                    let start = raw.find(token).unwrap();
                    assert!(
                        spans.iter().any(|span| span.range.start <= start
                            && start + token.len() <= span.range.end
                            && span.class == class),
                        "{raw:?}: {spans:?}"
                    );
                }
                let parse = ParseSnapshot::new(Source::new(&raw));
                let blocks = code_block::blocks(&parse);
                for span in &spans {
                    assert!(
                        raw.is_char_boundary(span.range.start)
                            && raw.is_char_boundary(span.range.end)
                    );
                    assert!(blocks[0]
                        .rows
                        .iter()
                        .any(|row| row.content.start <= span.range.start
                            && span.range.end <= row.content.end));
                }
            }
        }
    }

    #[test]
    fn cache_tracks_body_and_language_but_remaps_moved_blocks() {
        let mut highlighter = CodeHighlighter::default();
        let raw = "```rust\nlet n = 42;\n```\n";
        let first = spans(&mut highlighter, raw);
        assert_eq!(highlighter.parses, 1);
        assert_eq!(spans(&mut highlighter, raw), first);
        assert_eq!(highlighter.parses, 1);
        let shifted = spans(&mut highlighter, &format!("before\n\n{raw}"));
        assert_eq!(highlighter.parses, 1);
        assert_eq!(
            shifted[0].range.start,
            first[0].range.start + "before\n\n".len()
        );
        spans(&mut highlighter, &raw.replace("42", "\"hello\""));
        assert_eq!(highlighter.parses, 2);
        assert_eq!(highlighter.cache.len(), 1);
        spans(&mut highlighter, &raw.replace("rust", "python"));
        assert_eq!(highlighter.parses, 3);
        assert!(spans(&mut highlighter, &raw.replace("rust", "unknown")).is_empty());
        assert!(highlighter.cache.is_empty());
    }

    #[test]
    fn unknown_plain_empty_and_oversized_code_fall_back_without_parsing() {
        let mut highlighter = CodeHighlighter::default();
        for raw in [
            "```unknown\nlet n = 42;\n```",
            "```\nlet n = 42;\n```",
            "    let n = 42;\n",
            "```rust\n```",
            "```text\n**literal**\n```",
        ] {
            assert!(spans(&mut highlighter, raw).is_empty());
        }
        let raw = format!("```rust\n{}\n```", "a".repeat(MAX_BLOCK_BYTES));
        assert!(spans(&mut highlighter, &raw).is_empty());
        assert_eq!(highlighter.parses, 0);
        assert!(highlighter.configurations.is_empty());
    }
}
