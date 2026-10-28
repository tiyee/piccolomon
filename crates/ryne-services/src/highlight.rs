//! Code highlighting is a disposable view of Markdown source, never editable data.
pub use ryne_projection::Class;
use ryne_syntax::code_block::CodeBlock;
use std::{
    collections::HashMap,
    ops::Range,
    time::{Duration, Instant},
};
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
const MAX_DETECTION_ENTRIES: usize = 4096;
const DETECTION_BUDGET: Duration = Duration::from_millis(50);

const MAX_FULL_DETECTION_TIMEOUTS: u8 = 2;
#[derive(Clone, Copy)]
enum DetectionState {
    Complete(Option<Language>),
    Retry(u8),
    BudgetExhausted,
}
struct Detection {
    state: DetectionState,
    used: bool,
}

pub struct CodeHighlighter {
    highlighter: Highlighter,
    configurations: HashMap<Language, Option<HighlightConfiguration>>,
    cache: Vec<CachedBlock>,
    detections: HashMap<String, Detection>,
    detection_bytes: usize,
    max_detection_entries: usize,
    max_detection_bytes: usize,
    detection_attempts: usize,
    detection_remaining: Duration,
    detection_overflow: bool,
    deferred: bool,
    #[cfg(test)]
    parses: usize,
}
impl Default for CodeHighlighter {
    fn default() -> Self {
        Self {
            highlighter: Highlighter::default(),
            configurations: HashMap::new(),
            cache: Vec::new(),
            detections: HashMap::new(),
            detection_bytes: 0,
            max_detection_entries: MAX_DETECTION_ENTRIES,
            max_detection_bytes: MAX_CACHE_BYTES,
            detection_attempts: 0,
            detection_remaining: DETECTION_BUDGET,
            detection_overflow: false,
            deferred: false,
            #[cfg(test)]
            parses: 0,
        }
    }
}

impl CodeHighlighter {
    pub fn cache_entries(&self) -> usize {
        self.cache.len()
    }
    pub fn begin_frame(&mut self) {
        self.detection_attempts = 0;
        self.detection_remaining = DETECTION_BUDGET;
        self.detection_overflow = false;
        self.deferred = false;
        for detection in self.detections.values_mut() {
            detection.used = false;
        }
        for block in &mut self.cache {
            block.used = false;
        }
    }

    pub fn end_frame(&mut self) {
        self.cache.retain(|block| block.used);
        let before = self.detections.len();
        self.detections.retain(|text, detection| {
            if !detection.used {
                self.detection_bytes -= text.len();
            }
            detection.used
        });
        // Removed/changed blocks may free a slot after an earlier new block was
        // skipped. Retry once space is actually freed; a full current document
        // instead reaches a stable plain-code fallback without endless work.
        if self.detection_overflow && self.detections.len() < before {
            self.deferred = true;
        }
    }

    /// Return UTF-8 source ranges, excluding fences and container indentation.
    pub fn spans(&mut self, source: &str, block: &CodeBlock) -> Vec<Span> {
        let automatic = block
            .language
            .split_whitespace()
            .next()
            .is_some_and(|info| info.eq_ignore_ascii_case("auto"))
            || (block.language.trim().is_empty() && block.closing.is_some());
        let explicit = Language::from_info(&block.language);
        if !automatic && explicit.is_none() {
            return Vec::new();
        }
        let size: usize = block.rows.iter().map(|row| row.content.len() + 1).sum();
        if size == 0
            || size > MAX_BLOCK_BYTES
            || (automatic && size > crate::language::MAX_DETECTION_BYTES)
        {
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
        let language = if automatic {
            self.detect_language(&code, |code, deadline| {
                crate::language::TreeSitterLanguageDetector.detect_before(code, deadline)
            })
        } else {
            explicit
        };
        let Some(language) = language else {
            return Vec::new();
        };
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

    fn detect_language(
        &mut self,
        code: &str,
        detect: impl FnOnce(&str, Instant) -> crate::language::DetectionOutcome,
    ) -> Option<Language> {
        use crate::language::DetectionOutcome;
        let (timeouts, retry) = if let Some(cached) = self.detections.get_mut(code) {
            let already_used = cached.used;
            cached.used = true;
            match cached.state {
                DetectionState::Complete(language) => return language,
                DetectionState::BudgetExhausted => return None,
                DetectionState::Retry(count) => {
                    // Duplicate blocks sharing content get at most one attempt per pass.
                    if already_used {
                        self.deferred = true;
                        return None;
                    }
                    (count, true)
                }
            }
        } else {
            if code.len() > self.max_detection_bytes {
                return None;
            }
            if self.detections.len() >= self.max_detection_entries
                || self.detection_bytes + code.len() > self.max_detection_bytes
            {
                self.detection_overflow = true;
                return None;
            }
            (0, false)
        };
        // Retry with a fresh budget. A late block must not spend its finite
        // retry allowance repeatedly on the preceding blocks' leftover time.
        if self.detection_attempts >= 8
            || self.detection_remaining.is_zero()
            || (retry && self.detection_remaining < DETECTION_BUDGET)
        {
            self.deferred = true;
            return None;
        }
        self.detection_attempts += 1;
        let full_budget = self.detection_remaining == DETECTION_BUDGET;
        let started = Instant::now();
        let outcome = detect(code, started + self.detection_remaining);
        self.detection_remaining = self.detection_remaining.saturating_sub(started.elapsed());
        let state = match outcome {
            DetectionOutcome::Detected(info) => DetectionState::Complete(Language::from_info(info)),
            DetectionOutcome::Undetermined => DetectionState::Complete(None),
            DetectionOutcome::Deferred => {
                self.detection_remaining = Duration::ZERO;
                let timeouts = timeouts + u8::from(full_budget);
                if timeouts >= MAX_FULL_DETECTION_TIMEOUTS {
                    DetectionState::BudgetExhausted
                } else {
                    self.deferred = true;
                    DetectionState::Retry(timeouts)
                }
            }
        };
        if let Some(cached) = self.detections.get_mut(code) {
            cached.state = state;
        } else {
            self.detection_bytes += code.len();
            self.detections
                .insert(code.to_owned(), Detection { state, used: true });
        }
        match state {
            DetectionState::Complete(language) => language,
            _ => None,
        }
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
    fn has_pending_work(&self) -> bool {
        self.deferred
    }
    fn presentation_generation(&self) -> Option<u64> {
        (!self.deferred).then_some(0)
    }
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
    use editable_markdown_parser::Source;
    use ryne_syntax::code_block;
    use ryne_syntax::ParseSnapshot;

    #[test]
    fn partial_budget_timeout_retries_real_detection_with_a_fresh_budget() {
        let raw = "```\r\nfn main() { let s = \"中😀\"; }\r\n```\r\n";
        let parse = ParseSnapshot::new(Source::new(raw));
        let block = &code_block::cached(&parse)[0];
        let mut h = CodeHighlighter::default();
        h.begin_frame();
        h.detection_remaining = Duration::from_nanos(1);
        assert!(h.spans(raw, block).is_empty());
        // Repeated identical blocks cannot consume retries within the same pass.
        assert!(h.spans(raw, block).is_empty());
        assert_eq!(h.detection_attempts, 1);
        h.end_frame();
        assert!(h.deferred);
        assert!(matches!(
            h.detections.values().next().unwrap().state,
            DetectionState::Retry(0)
        ));
        h.begin_frame();
        assert!(!h.spans(raw, block).is_empty());
        h.end_frame();
        assert!(!h.deferred);
        assert_eq!(h.detection_attempts, 1);
    }

    #[test]
    fn retries_use_full_budgets_and_persistent_timeouts_converge_without_starving_later_code() {
        use crate::language::DetectionOutcome::{Deferred, Detected};
        let mut h = CodeHighlighter::default();
        h.begin_frame();
        h.detection_remaining = Duration::from_nanos(1);
        assert!(h.detect_language("slow", |_, _| Deferred).is_none());
        h.end_frame();
        for attempt in 1..=MAX_FULL_DETECTION_TIMEOUTS {
            h.begin_frame();
            assert!(h.detect_language("slow", |_, _| Deferred).is_none());
            assert!(h
                .detect_language("later", |_, _| panic!("spent budget"))
                .is_none());
            h.end_frame();
            assert!(h.deferred); // later content still needs a pass
            assert!(
                matches!(h.detections["slow"].state, DetectionState::BudgetExhausted)
                    == (attempt == MAX_FULL_DETECTION_TIMEOUTS)
            );
        }
        h.begin_frame();
        assert!(h
            .detect_language("slow", |_, _| panic!("terminal content retried"))
            .is_none());
        assert!(h
            .detect_language("later", |_, _| Detected("rust"))
            .is_some());
        h.end_frame();
        assert!(!h.deferred);
        assert_eq!(h.detection_attempts, 1);
        // Removal/cancellation of content releases retry/terminal records and bytes.
        h.begin_frame();
        h.end_frame();
        assert!(h.detections.is_empty());
        assert_eq!(h.detection_bytes, 0);
        h.begin_frame();
        assert!(h.detect_language("slow", |_, _| Detected("rust")).is_some());
        h.end_frame();
        assert!(!h.deferred);
    }

    #[test]
    fn reordered_and_changed_content_keeps_retry_allowance_and_negative_cache_distinct() {
        use crate::language::DetectionOutcome::{Deferred, Detected, Undetermined};
        let mut h = CodeHighlighter::default();
        h.begin_frame();
        h.detection_remaining = Duration::from_nanos(1);
        h.detect_language("pending", |_, _| Deferred);
        h.end_frame();
        h.begin_frame();
        h.detect_language("new earlier", |_, _| Undetermined);
        h.detection_remaining = Duration::from_millis(49);
        h.detect_language("pending", |_, _| panic!("must wait for a full budget"));
        h.end_frame();
        assert!(h.deferred);
        assert!(matches!(
            h.detections["pending"].state,
            DetectionState::Retry(0)
        ));
        h.begin_frame();
        // Completed ambiguity never needs another inference attempt.
        h.detect_language("new earlier", |_, _| panic!("negative cache miss"));
        assert!(h
            .detect_language("pending", |_, _| Detected("python"))
            .is_some());
        h.end_frame();
        assert!(!h.deferred);
        h.begin_frame();
        h.detect_language("changed body", |_, _| Detected("rust"));
        h.end_frame();
        assert_eq!(h.detections.len(), 1);
        assert!(h.detections.contains_key("changed body"));
    }

    #[test]
    fn first_batch_many_automatic_blocks_eventually_complete() {
        for count in [8usize, 64, 80, 128] {
            let raw = (0..count)
                .map(|i| format!("```\nfn main_{i}() {{}}\n```\n\n"))
                .collect::<String>();
            let parse = ParseSnapshot::new(Source::new(&raw));
            let blocks = code_block::blocks(&parse);
            let mut highlighter = CodeHighlighter::default();
            let mut complete = false;
            for _ in 0..count + 2 {
                highlighter.begin_frame();
                let colored = blocks
                    .iter()
                    .filter(|b| !highlighter.spans(&raw, b).is_empty())
                    .count();
                highlighter.end_frame();
                if ryne_extensions::SyntaxHighlighter::presentation_generation(&highlighter)
                    .is_some()
                {
                    assert_eq!(colored, count);
                    complete = true;
                    break;
                }
            }
            assert!(complete, "{count} automatic blocks did not converge");
        }
    }

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
    fn automatic_highlighting_caches_success_failure_and_remaps_without_source_edits() {
        let mut highlighter = CodeHighlighter::default();
        for info in ["", "auto", "AUTO extra"] {
            let raw = format!("```{info}\r\nfn main() {{ let s = \"中😀\"; }}\r\n```\r\n");
            let first = spans(&mut highlighter, &raw);
            assert!(first.iter().any(|s| s.class == Class::Keyword));
            assert_eq!(highlighter.detections.len(), 1);
            let shifted = spans(&mut highlighter, &format!("before\r\n\r\n{raw}"));
            assert_eq!(shifted[0].range.start, first[0].range.start + 10);
            assert_eq!(highlighter.detection_attempts, 0);
        }
        assert!(spans(&mut highlighter, "```auto\nx = 1\n```").is_empty());
        assert_eq!(highlighter.detections.len(), 1);
        assert!(spans(&mut highlighter, "```auto\nx = 1\n```").is_empty());
        assert_eq!(highlighter.detection_attempts, 0);
        assert!(spans(
            &mut highlighter,
            &format!(
                "```auto\n{}\n```",
                "x".repeat(crate::language::MAX_DETECTION_BYTES)
            )
        )
        .is_empty());
        assert!(highlighter.detections.is_empty());
    }

    #[test]
    fn detection_limits_finish_with_plain_code_and_reclaim_removed_content() {
        for (entries, bytes) in [(4, 2048), (4096, 40)] {
            let raw = (0..12)
                .map(|i| format!("```\nfn main_{i}() {{}}\n```\n\n"))
                .collect::<String>();
            let parse = ParseSnapshot::new(Source::new(&raw));
            let blocks = code_block::blocks(&parse);
            let mut h = CodeHighlighter {
                max_detection_entries: entries,
                max_detection_bytes: bytes,
                ..Default::default()
            };
            for _ in 0..16 {
                h.begin_frame();
                for block in &blocks {
                    h.spans(&raw, block);
                }
                h.end_frame();
                if !h.deferred {
                    break;
                }
            }
            assert!(!h.deferred);
            assert!(h.detections.len() <= entries);
            assert!(h.detection_bytes <= bytes);
            let retained = h.detections.len();
            assert!(retained > 0 && retained < 12);
            let changed = "```\nfn replacement() {}\n```\n";
            let changed_parse = ParseSnapshot::new(Source::new(changed));
            let block = &code_block::blocks(&changed_parse)[0];
            for _ in 0..3 {
                h.begin_frame();
                h.spans(changed, block);
                h.end_frame();
                if !h.deferred {
                    break;
                }
            }
            assert!(!h.deferred);
            assert_eq!(h.detections.len(), 1);
            assert_eq!(h.detection_bytes, "fn replacement() {}\n".len());
        }
    }

    #[test]
    fn shared_deadline_defers_without_spending_attempts_and_next_pass_resumes() {
        let raw = "```\nfn main() {}\n```\n";
        let parse = ParseSnapshot::new(Source::new(raw));
        let block = &code_block::blocks(&parse)[0];
        let mut h = CodeHighlighter::default();
        h.begin_frame();
        let explicit = raw.replacen("```", "```rust", 1);
        let explicit_parse = ParseSnapshot::new(Source::new(&explicit));
        h.spans(&explicit, &code_block::blocks(&explicit_parse)[0]);
        assert_eq!(h.detection_remaining, DETECTION_BUDGET);
        assert_eq!(h.detection_attempts, 0);
        h.detection_remaining = Duration::ZERO;
        assert!(h.spans(raw, block).is_empty());
        h.end_frame();
        assert!(h.deferred);
        assert_eq!(h.detection_attempts, 0);
        h.begin_frame();
        assert!(!h.spans(raw, block).is_empty());
        h.end_frame();
        assert!(!h.deferred);
        assert_eq!(h.detection_attempts, 1);
    }

    #[test]
    fn unknown_plain_empty_and_oversized_code_fall_back_without_parsing() {
        let mut highlighter = CodeHighlighter::default();
        for raw in [
            "```unknown\nlet n = 42;\n```",
            "```\nx = 1\n```",
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
