//! Language choices are fence edits; code bodies remain literal source.
use crate::commands::Planner;
use ryne_extensions::*;
use ryne_syntax::code_block;
use std::{ops::Range, sync::Arc};

pub(crate) struct CodeActions {
    pub formulas: Arc<dyn FormulaRenderer>,
}
const CHOICES: &[(&str, &str, &str)] = &[
    ("code.language-auto", "auto", ""),
    ("code.language-rust", "Rust", "rust"),
    ("code.language-python", "Python", "python"),
    ("code.language-javascript", "JavaScript / JSX", "javascript"),
    ("code.language-typescript", "TypeScript", "typescript"),
    ("code.language-tsx", "TSX", "tsx"),
    ("code.language-json", "JSON", "json"),
    ("code.language-bash", "Bash / Shell", "bash"),
    ("code.language-c", "C", "c"),
    ("code.language-cpp", "C++", "cpp"),
    ("code.language-java", "Java", "java"),
    ("code.language-go", "Go", "go"),
    ("code.language-html", "HTML", "html"),
    ("code.language-css", "CSS", "css"),
    ("code.language-sql", "SQL", "sql"),
    ("code.language-yaml", "YAML", "yaml"),
    ("code.language-mermaid", "Mermaid", "mermaid"),
];
const COMMANDS: &[&str] = &[
    "code.language-auto",
    "code.language-rust",
    "code.language-python",
    "code.language-javascript",
    "code.language-typescript",
    "code.language-tsx",
    "code.language-json",
    "code.language-bash",
    "code.language-c",
    "code.language-cpp",
    "code.language-java",
    "code.language-go",
    "code.language-html",
    "code.language-css",
    "code.language-sql",
    "code.language-yaml",
    "code.language-mermaid",
];
fn canonical(info: &str) -> &str {
    match info.split_whitespace().next().unwrap_or("") {
        "" | "auto" => "",
        "text" | "txt" | "plain" | "plaintext" => "text",
        "rs" => "rust",
        "py" | "python3" => "python",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "ts" | "mts" | "cts" => "typescript",
        "sh" | "shell" => "bash",
        "jsonc" => "json",
        other => other,
    }
}
/// Locate only the info token, preserving fence width, spacing and trailing metadata.
fn language_range(state: &EditingState, block: &code_block::CodeBlock) -> Option<Range<usize>> {
    if block
        .rows
        .first()
        .is_some_and(|row| row.line.start == block.range.start)
    {
        return None; // Indented code can contain literal fence-looking text.
    }
    let normalized = ryne_syntax::parser::normalized(&state.parse);
    let parse = normalized.unwrap_or(&state.parse);
    let node = parse.nodes.iter().find(|n| {
        matches!(n.kind, editable_markdown_parser::NodeKind::CodeBlock(_))
            && block.range.contains(&n.envelope.start)
    })?;
    let text = state.source.text();
    let start = node.envelope.start;
    let line = &text[start..];
    let marker = *line.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let width = line.bytes().take_while(|b| *b == marker).count();
    if width < 3 {
        return None;
    }
    let end = start + line.find(['\r', '\n']).unwrap_or(line.len());
    let rest = &text[start + width..end];
    let token = start + width + rest.len() - rest.trim_start_matches([' ', '\t']).len();
    let len = text[token..end]
        .find(char::is_whitespace)
        .unwrap_or(end - token);
    Some(token..token + len)
}
impl RenderActionProvider for CodeActions {
    fn cache_dependencies(&self) -> Option<CacheDependencies> {
        Some(CacheDependencies::default())
    }
    fn id(&self) -> &'static str {
        "code"
    }
    fn commands(&self) -> &'static [&'static str] {
        COMMANDS
    }
    fn controls(&self, state: &EditingState) -> Result<Vec<RenderControl>, String> {
        Ok(code_block::blocks(&state.parse)
            .into_iter()
            .filter_map(|block| {
                language_range(state, &block)?;
                let lower = block.language.to_ascii_lowercase();
                let selected = canonical(&lower);
                let label = CHOICES.iter().find(|c| c.2 == selected).map_or_else(
                    || {
                        block
                            .language
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(48)
                            .collect()
                    },
                    |c| c.1.to_owned(),
                );
                Some(RenderControl {
                    provider: "code",
                    id: block.range.start.to_string(),
                    target: RenderTarget {
                        document: state.source.id(),
                        role: "code.block",
                        owner: block.range.clone(),
                        part: vec![],
                    },
                    focus: block.anchor..block.rows.last().map_or(block.anchor, |r| r.content.end),
                    placement: RenderControlPlacement::Header { label },
                    trailing_space: 0.,
                    actions: CHOICES
                        .iter()
                        .map(|&(command, fallback, info)| RenderAction {
                            command,
                            label: command,
                            fallback,
                            icon: None,
                            enabled: true,
                            checked: selected == info,
                            group: u8::from(!info.is_empty()),
                            submenu: None,
                            shortcut: None,
                        })
                        .collect(),
                })
            })
            .collect())
    }
    fn plan(
        &self,
        state: &EditingState,
        request: &RenderActionRequest,
    ) -> Result<CommandPlan, String> {
        request.target.validate(state)?;
        if request.target.role != "code.block" || !request.target.part.is_empty() {
            return Err("invalid code target".into());
        }
        let block = code_block::blocks(&state.parse)
            .into_iter()
            .find(|b| b.range == request.target.owner)
            .ok_or("code block disappeared")?;
        let mut range = language_range(state, &block).ok_or("code has no editable fence")?;
        let info = CHOICES
            .iter()
            .find(|c| c.0 == request.command)
            .ok_or("unknown language")?
            .2;
        if (info.is_empty() && block.language.trim().is_empty())
            || (!info.is_empty() && canonical(&block.language.to_ascii_lowercase()) == info)
        {
            return Ok(CommandPlan::Consume);
        }
        if info.is_empty() {
            // All info must be empty: otherwise trailing metadata becomes the new language.
            let rest = &state.source.text()[range.start..];
            let end = rest.find(['\r', '\n']).unwrap_or(rest.len());
            range.end = range.start + rest[..end].trim_end_matches([' ', '\t']).len();
        }
        let mut planner = Planner {
            state: state.clone(),
            prepared: None,
            renderer: self.formulas.as_ref(),
            mapping: None,
        };
        planner
            .apply_edits(
                vec![editable_markdown_parser::Edit {
                    range,
                    replacement: info.into(),
                }],
                None,
            )
            .map_err(|e| format!("{e:?}"))?;
        Ok(match planner.prepared {
            Some(prepared) => CommandPlan::Edit {
                prepared,
                state: planner.state,
                handled: true,
            },
            None => CommandPlan::Consume,
        })
    }
}
