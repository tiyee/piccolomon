//! Bounded, conservative inference using the same grammars as highlighting.
use ryne_extensions::CodeLanguageDetector;
use std::{
    ops::ControlFlow,
    time::{Duration, Instant},
};
use tree_sitter::{Language, ParseOptions, Parser};

pub const MAX_DETECTION_BYTES: usize = 32 * 1024;

pub struct TreeSitterLanguageDetector;

impl CodeLanguageDetector for TreeSitterLanguageDetector {
    fn detect(&self, code: &str) -> Option<&'static str> {
        if code.trim().is_empty() || code.len() > MAX_DETECTION_BYTES {
            return None;
        }
        let deadline = Instant::now() + Duration::from_millis(50);
        let mut parser = Parser::new();
        let mut found = Vec::new();
        for (name, grammar) in [
            ("rust", Language::from(tree_sitter_rust::LANGUAGE)),
            ("python", tree_sitter_python::LANGUAGE.into()),
            ("javascript", tree_sitter_javascript::LANGUAGE.into()),
            (
                "typescript",
                tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            ),
            ("tsx", tree_sitter_typescript::LANGUAGE_TSX.into()),
            ("json", tree_sitter_json::LANGUAGE.into()),
            ("bash", tree_sitter_bash::LANGUAGE.into()),
        ] {
            if Instant::now() >= deadline {
                return None;
            }
            parser.set_language(&grammar).ok()?;
            let mut progress = |_: &tree_sitter::ParseState| {
                if Instant::now() >= deadline {
                    ControlFlow::Break(())
                } else {
                    ControlFlow::Continue(())
                }
            };
            let tree = parser.parse_with_options(
                &mut |offset, _| &code.as_bytes()[offset..],
                None,
                Some(ParseOptions::new().progress_callback(&mut progress)),
            )?;
            let root = tree.root_node();
            if root.has_error() {
                continue;
            }
            let mut strong = false;
            let mut jsx = false;
            let mut stack = vec![root];
            while let Some(node) = stack.pop() {
                if Instant::now() >= deadline {
                    return None;
                }
                let kind = node.kind();
                jsx |= matches!(kind, "jsx_element" | "jsx_self_closing_element");
                strong |= match name {
                    "rust" => matches!(
                        kind,
                        "function_item"
                            | "struct_item"
                            | "enum_item"
                            | "impl_item"
                            | "trait_item"
                            | "use_declaration"
                    ),
                    "python" => matches!(
                        kind,
                        "function_definition"
                            | "class_definition"
                            | "import_statement"
                            | "import_from_statement"
                    ),
                    "javascript" => {
                        matches!(
                            kind,
                            "lexical_declaration"
                                | "variable_declaration"
                                | "function_declaration"
                                | "arrow_function"
                                | "import_statement"
                                | "export_statement"
                        ) || jsx
                    }
                    "typescript" | "tsx" => matches!(
                        kind,
                        "interface_declaration"
                            | "type_alias_declaration"
                            | "type_annotation"
                            | "enum_declaration"
                    ),
                    "json" => {
                        node.parent().is_some_and(|p| p == root)
                            && matches!(kind, "object" | "array")
                    }
                    "bash" => {
                        kind == "command_name"
                            && matches!(
                                node.utf8_text(code.as_bytes()).ok(),
                                Some(
                                    "echo"
                                        | "printf"
                                        | "export"
                                        | "source"
                                        | "cd"
                                        | "mkdir"
                                        | "curl"
                                        | "grep"
                                        | "sed"
                                        | "awk"
                                        | "git"
                                        | "cargo"
                                        | "npm"
                                        | "pip"
                                )
                            )
                            && node.parent().is_some_and(|p| p.child_count() > 1)
                    }
                    _ => false,
                };
                for index in 0..node.named_child_count() {
                    if let Some(child) = node.named_child(index as u32) {
                        stack.push(child);
                    }
                }
            }
            if name == "bash" {
                let first = code.lines().next().unwrap_or("");
                strong |= first.strip_prefix("#!").is_some_and(|s| {
                    matches!(
                        s.trim(),
                        "/bin/bash" | "/bin/sh" | "/usr/bin/env bash" | "/usr/bin/env sh"
                    )
                });
            }
            if strong && (name != "tsx" || jsx) {
                found.push(name);
            }
        }
        // TypeScript accepts JavaScript too; require actual type syntax above.
        if found.contains(&"tsx") {
            found.retain(|name| !matches!(*name, "javascript" | "typescript"));
        } else if found.contains(&"typescript") {
            found.retain(|name| *name != "javascript");
        }
        (found.len() == 1).then(|| found[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distinctive_valid_code_uses_bundled_languages() {
        for (code, expected) in [
            ("fn main() { let s = \"中😀\"; }", "rust"),
            ("def greet(name):\n    return name", "python"),
            ("const n = 42;", "javascript"),
            ("const view = <main />;", "javascript"),
            ("interface User { name: string }", "typescript"),
            ("const view: View = <main />;", "tsx"),
            ("{\"name\": \"中😀\", \"n\": 42}", "json"),
            ("echo \"hello\"", "bash"),
        ] {
            assert_eq!(
                TreeSitterLanguageDetector.detect(code),
                Some(expected),
                "{code}"
            );
        }
    }
    #[test]
    fn ambiguous_literal_incomplete_and_oversized_input_stays_unlabelled() {
        for code in [
            "",
            "hello world",
            "x = 1",
            "print(42)",
            "42",
            "**literal**",
            "\"fn main() {}\"",
            "// fn main() {}",
            "fn main( {",
            "def greet(",
        ] {
            assert_eq!(TreeSitterLanguageDetector.detect(code), None, "{code}");
        }
        assert_eq!(
            TreeSitterLanguageDetector.detect(&"x".repeat(MAX_DETECTION_BYTES + 1)),
            None
        );
    }
}
