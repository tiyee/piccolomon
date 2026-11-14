//! A disposable document model for testing tree edits and Markdown round trips.

use std::{collections::BTreeMap, io::Write, ops::Range, path::Path, sync::Arc};

use pulldown_cmark::{Event, Options, Parser, Tag};
use unicode_segmentation::UnicodeSegmentation;

const MARKDOWN: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_STRIKETHROUGH)
    .union(Options::ENABLE_MATH)
    .union(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS)
    .union(Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS);

type NodeId = usize;
type Children = Vec<Arc<Node>>;
type Definitions = BTreeMap<String, (String, Option<String>)>;

#[derive(Clone, Debug)]
pub struct Node {
    pub id: NodeId,
    pub kind: Kind,
}

#[derive(Clone, Debug)]
pub enum Kind {
    Container(Tag<'static>, Children),
    Leaf(Event<'static>),
}

pub struct Document {
    blocks: Children,
    definitions: Definitions,
    next_id: NodeId,
    undo: Vec<Children>,
}

impl Document {
    pub fn open(markdown: &str) -> Self {
        let parser = Parser::new_ext(markdown, MARKDOWN);
        let definitions = definitions(&parser);
        let mut blocks = Vec::new();
        let mut stack = Vec::new();
        let mut next_id = 0;
        for event in parser.map(Event::into_static) {
            let node = match event {
                Event::Start(tag) => {
                    stack.push((next_id, tag, Vec::new()));
                    next_id += 1;
                    continue;
                }
                Event::End(_) => {
                    let (id, tag, children) = stack.pop().expect("balanced parser events");
                    Node {
                        id,
                        kind: Kind::Container(tag, children),
                    }
                }
                leaf => {
                    let id = next_id;
                    next_id += 1;
                    Node {
                        id,
                        kind: Kind::Leaf(leaf),
                    }
                }
            };
            let target = stack
                .last_mut()
                .map_or(&mut blocks, |(_, _, children)| children);
            target.push(Arc::new(node));
        }
        Self {
            blocks,
            definitions,
            next_id,
            undo: Vec::new(),
        }
    }

    pub fn blocks(&self) -> &[Arc<Node>] {
        &self.blocks
    }

    /// Replace bytes in a single text node. A real editor must also check graphemes
    /// spanning adjacent text runs when mapping selections to this operation.
    pub fn replace_text(
        &mut self,
        id: NodeId,
        range: Range<usize>,
        text: &str,
    ) -> Result<(), &'static str> {
        let mut path = Vec::new();
        let node = find(&self.blocks, id, &mut path).ok_or("unknown node")?;
        let Kind::Leaf(Event::Text(current)) = &node.kind else {
            return Err("expected a text node");
        };
        let boundary = |offset| {
            offset == current.len() || current.grapheme_indices(true).any(|(i, _)| i == offset)
        };
        if range.start > range.end
            || !boundary(range.start)
            || !boundary(range.end)
            || text.contains(['\r', '\n'])
        {
            return Err("expected a single text run and grapheme boundaries");
        }
        if &current[range.clone()] == text {
            return Ok(());
        }
        let mut replacement = current.to_string();
        replacement.replace_range(range, text);
        let mut blocks = self.blocks.clone();
        let (index, parents) = path.split_last().expect("a node has an index");
        let siblings = children_mut(&mut blocks, parents);
        if replacement.is_empty() {
            siblings.remove(*index);
        } else {
            siblings[*index] = Arc::new(Node {
                id,
                kind: Kind::Leaf(Event::Text(replacement.into())),
            });
        }
        self.commit(blocks);
        Ok(())
    }

    /// Move a list item under its preceding sibling, creating a nested list as needed.
    pub fn indent_item(&mut self, id: NodeId) -> Result<(), &'static str> {
        let mut path = Vec::new();
        let node = find(&self.blocks, id, &mut path).ok_or("unknown node")?;
        if !matches!(node.kind, Kind::Container(Tag::Item, _)) {
            return Err("expected a list item");
        }
        let (&index, parents) = path.split_last().expect("a node has an index");
        let Some((&list_index, ancestors)) = parents.split_last() else {
            return Err("expected a parent list");
        };
        if index == 0 {
            return Err("the first item has no preceding sibling");
        }
        let mut blocks = self.blocks.clone();
        let list = Arc::make_mut(&mut children_mut(&mut blocks, ancestors)[list_index]);
        let Kind::Container(Tag::List(number), items) = &mut list.kind else {
            return Err("expected a parent list");
        };
        let moved = items.remove(index);
        let Kind::Container(Tag::Item, previous) = &mut Arc::make_mut(&mut items[index - 1]).kind
        else {
            return Err("expected a preceding list item");
        };
        let nested = previous.last_mut().and_then(|last| match &last.kind {
            Kind::Container(Tag::List(start), _) if start.is_some() == number.is_some() => {
                Some(last)
            }
            _ => None,
        });
        if let Some(nested) = nested {
            let Kind::Container(_, children) = &mut Arc::make_mut(nested).kind else {
                unreachable!()
            };
            children.push(moved);
        } else {
            previous.push(Arc::new(Node {
                id: self.next_id,
                kind: Kind::Container(Tag::List(number.map(|_| 1)), vec![moved]),
            }));
            self.next_id += 1;
        }
        self.commit(blocks);
        Ok(())
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.blocks = previous;
        true
    }

    /// Return Markdown only if the output preserves the tree and link definitions.
    /// This deliberately runs the full verification on save, outside editing.
    pub fn markdown_for_save(&self) -> Result<String, String> {
        let mut expected = Vec::new();
        for block in &self.blocks {
            emit(block, &mut expected);
        }
        let mut output = String::new();
        pulldown_cmark_to_cmark::cmark(&expected, &mut output, MARKDOWN)
            .map_err(|error| error.to_string())?;
        let parser = Parser::new_ext(&output, MARKDOWN);
        let actual_definitions = definitions(&parser);
        if self
            .definitions
            .iter()
            .any(|(label, definition)| actual_definitions.get(label) != Some(definition))
        {
            return Err("serialization would lose or change a reference definition".into());
        }
        if merge_text(expected) != merge_text(parser.map(Event::into_static)) {
            return Err("serialization would change the document structure or content".into());
        }
        Ok(output)
    }

    /// The probe only creates new files so evaluation cannot overwrite originals.
    pub fn save_new(&self, path: &Path) -> std::io::Result<()> {
        let output = self.markdown_for_save().map_err(std::io::Error::other)?;
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(output.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist_noclobber(path)?;
        Ok(())
    }

    fn commit(&mut self, blocks: Children) {
        self.undo.push(std::mem::replace(&mut self.blocks, blocks));
    }
}

fn find<'a>(nodes: &'a [Arc<Node>], id: NodeId, path: &mut Vec<usize>) -> Option<&'a Node> {
    for (index, node) in nodes.iter().enumerate() {
        path.push(index);
        if node.id == id {
            return Some(node);
        }
        if let Kind::Container(_, children) = &node.kind
            && let Some(found) = find(children, id, path)
        {
            return Some(found);
        }
        path.pop();
    }
    None
}

fn children_mut<'a>(nodes: &'a mut Children, path: &[usize]) -> &'a mut Children {
    let Some((&index, rest)) = path.split_first() else {
        return nodes;
    };
    let Kind::Container(_, children) = &mut Arc::make_mut(&mut nodes[index]).kind else {
        unreachable!("paths descend only into containers");
    };
    children_mut(children, rest)
}

fn emit(node: &Node, out: &mut Vec<Event<'static>>) {
    match &node.kind {
        Kind::Container(tag, children) => {
            out.push(Event::Start(tag.clone()));
            for child in children {
                emit(child, out);
            }
            out.push(Event::End(tag.to_end()));
        }
        Kind::Leaf(event) => out.push(event.clone()),
    }
}

fn definitions(parser: &Parser<'_>) -> Definitions {
    parser
        .reference_definitions()
        .iter()
        .map(|(label, definition)| {
            (
                label.to_owned(),
                (
                    definition.dest.to_string(),
                    definition.title.as_ref().map(ToString::to_string),
                ),
            )
        })
        .collect()
}

fn merge_text(events: impl IntoIterator<Item = Event<'static>>) -> Vec<Event<'static>> {
    let mut merged = Vec::new();
    for event in events {
        match (merged.last_mut(), event) {
            (Some(Event::Text(previous)), Event::Text(text)) => {
                let mut joined = std::mem::replace(previous, "".into()).into_string();
                joined.push_str(&text);
                *previous = joined.into();
            }
            (_, event) => merged.push(event),
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    const NESTED: &str = include_str!("nested.md");

    fn node_matching(nodes: &[Arc<Node>], predicate: impl Fn(&Kind) -> bool + Copy) -> &Node {
        fn search(nodes: &[Arc<Node>], predicate: impl Fn(&Kind) -> bool + Copy) -> Option<&Node> {
            for node in nodes {
                if predicate(&node.kind) {
                    return Some(node);
                }
                if let Kind::Container(_, children) = &node.kind
                    && let Some(found) = search(children, predicate)
                {
                    return Some(found);
                }
            }
            None
        }
        search(nodes, predicate).expect("node in fixture")
    }

    #[test]
    fn round_trips_existing_markdown_features() {
        let doc = Document::open(NESTED);
        let saved = doc.markdown_for_save().expect("semantic round trip");
        assert_ne!(saved, NESTED);
        assert_eq!(Document::open(&saved).markdown_for_save().unwrap(), saved);
    }

    #[test]
    fn edits_text_and_indents_a_list_item_inside_a_quote_then_undoes() {
        let mut doc = Document::open(NESTED);
        let baseline = doc.markdown_for_save().unwrap();
        let heading = Arc::clone(&doc.blocks()[1]);
        let text = node_matching(
            doc.blocks(),
            |kind| matches!(kind, Kind::Leaf(Event::Text(text)) if text.starts_with("first ")),
        );
        let text_id = text.id;
        doc.replace_text(text_id, 0..5, "中文").unwrap();
        let edited = doc.markdown_for_save().unwrap();
        assert!(edited.contains("中文"));
        let list = node_matching(doc.blocks(), |kind| {
            matches!(kind, Kind::Container(Tag::List(Some(3)), _))
        });
        let Kind::Container(_, items) = &list.kind else {
            unreachable!()
        };
        let moved = Arc::clone(&items[1]);
        doc.indent_item(moved.id).unwrap();
        let list = node_matching(doc.blocks(), |kind| {
            matches!(kind, Kind::Container(Tag::List(Some(3)), _))
        });
        let Kind::Container(_, items) = &list.kind else {
            unreachable!()
        };
        assert_eq!(items.len(), 1);
        let nested = node_matching(&items[0..1], |kind| {
            matches!(kind, Kind::Container(Tag::List(Some(1)), _))
        });
        let Kind::Container(_, children) = &nested.kind else {
            unreachable!()
        };
        assert!(Arc::ptr_eq(&children[0], &moved));
        assert!(Arc::ptr_eq(&doc.blocks()[1], &heading));
        let saved = doc.markdown_for_save().unwrap();
        assert_ne!(saved, edited);
        assert_eq!(Document::open(&saved).markdown_for_save().unwrap(), saved);
        assert!(doc.undo());
        assert_eq!(doc.markdown_for_save().unwrap(), edited);
        assert!(doc.undo());
        assert_eq!(doc.markdown_for_save().unwrap(), baseline);
        assert!(!doc.undo());
    }

    #[test]
    fn unused_reference_definition_cannot_silently_disappear() {
        let doc = Document::open("[unused]: /path \"Title\"\n\nContent");
        assert!(doc
            .markdown_for_save()
            .unwrap_err()
            .contains("reference definition"));
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("rejected.md");
        assert!(doc.save_new(&output).is_err());
        assert!(!output.exists());
    }

    #[test]
    fn saves_to_disk_and_reopens_without_overwriting_an_existing_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("nested.md");
        let doc = Document::open(NESTED);
        doc.save_new(&path).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        let reopened = Document::open(&written);
        assert_eq!(reopened.markdown_for_save().unwrap(), written);
        let error = Document::open("Replacement").save_new(&path).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_to_string(path).unwrap(), written);
    }

    #[test]
    fn text_edit_rejects_splitting_a_grapheme() {
        let mut doc = Document::open("A e\u{301}");
        let text_id = node_matching(doc.blocks(), |kind| {
            matches!(kind, Kind::Leaf(Event::Text(_)))
        })
        .id;
        assert!(doc.replace_text(text_id, 3..3, "X").is_err());
        assert!(!doc.undo());
    }
}
