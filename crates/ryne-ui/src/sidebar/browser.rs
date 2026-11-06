use ryne_extensions::directory::*;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::{Path, PathBuf},
    time::SystemTime,
};

/// Paths are relative to the display root. Native names also break sort ties.
#[derive(Default)]
pub(super) struct Browser {
    pub root: Option<PathBuf>,
    pub manual: bool,
    pub current: Option<PathBuf>,
    pub generation: u64,
    pub expanded: HashSet<PathBuf>,
    pub entries: HashMap<PathBuf, DirectoryEntry>,
    children: BTreeMap<PathBuf, BTreeSet<(bool, PathBuf)>>,
    documents: BTreeSet<(std::cmp::Reverse<Option<SystemTime>>, PathBuf)>,
    unread: BTreeSet<(usize, PathBuf)>,
    // Finished attempts, including failures/limits. Only a complete scan can prove emptiness.
    done: HashMap<PathBuf, Option<ScanEnd>>,
    pub errors: BTreeMap<PathBuf, String>,
    pub partial: bool,
    examined: usize,
    tree_next: Option<PathBuf>,
    pub tree: Vec<PathBuf>,
    pub list: Vec<PathBuf>,
    pub dirty_rows: bool,
    pub filter: String,
}

impl Browser {
    pub fn root(&mut self, root: Option<PathBuf>, manual: bool, refresh: bool) -> bool {
        self.manual = manual;
        if self.root == root && !refresh {
            return false;
        }
        let expanded = if refresh {
            std::mem::take(&mut self.expanded)
        } else {
            Default::default()
        };
        let filter = if refresh {
            std::mem::take(&mut self.filter)
        } else {
            String::new()
        };
        *self = Self {
            root,
            manual,
            generation: self.generation.wrapping_add(1),
            current: self.current.take(),
            expanded,
            filter,
            dirty_rows: true,
            ..Self::default()
        };
        self.unread.insert((0, PathBuf::new()));
        true
    }
    pub fn installed(&mut self, path: Option<&Path>) -> bool {
        self.current = path.and_then(|p| std::path::absolute(p).ok());
        if self.current.is_none() && self.manual {
            return false;
        }
        self.root(
            self.current
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_owned),
            false,
            false,
        )
    }
    pub fn done_parent(&self, path: &Path) -> bool {
        path.parent().is_some_and(|p| self.done.contains_key(p))
    }
    pub fn known_empty(&self, path: &Path) -> bool {
        matches!(
            self.done.get(path),
            Some(Some(ScanEnd {
                status: ScanStatus::Complete,
                examined: 0
            }))
        ) && !self.children.contains_key(path)
    }
    pub fn relative_current(&self) -> Option<&Path> {
        self.current
            .as_ref()?
            .strip_prefix(self.root.as_ref()?)
            .ok()
    }
    pub fn insert(&mut self, dir: &Path, entry: DirectoryEntry) {
        // A replaceable port cannot supply absolute names or parent traversal.
        if Path::new(&entry.name).file_name() != Some(entry.name.as_os_str()) {
            return;
        }
        let path = dir.join(&entry.name);
        let document = entry.kind == EntryKind::Document;
        let is_current = self.relative_current() == Some(path.as_path());
        if !self.entries.contains_key(&path)
            && self.entries.len()
                >= ENTRY_LIMIT
                    - usize::from(
                        !is_current
                            && self
                                .relative_current()
                                .is_some_and(|p| !self.entries.contains_key(p)),
                    )
        {
            self.partial = true;
            return;
        }
        let depth = path.components().count();
        if let Some(old) = self.entries.get(&path) {
            self.documents
                .remove(&(std::cmp::Reverse(old.modified), path.clone()));
            if (old.kind == EntryKind::Document) != document
                && let Some(children) = self.children.get_mut(dir)
            {
                children.remove(&(old.kind == EntryKind::Document, path.clone()));
            }
        }
        if document {
            self.documents
                .insert((std::cmp::Reverse(entry.modified), path.clone()));
        }
        if entry.kind == EntryKind::Directory && !self.done.contains_key(&path) {
            if depth <= MAX_DEPTH {
                self.unread.insert((depth, path.clone()));
            } else {
                self.partial = true
            }
        }
        self.children
            .entry(dir.to_owned())
            .or_default()
            .insert((document, path.clone()));
        self.entries.insert(path, entry);
        self.dirty_rows = true;
    }
    pub fn finish(&mut self, path: &Path, result: Result<ScanEnd, String>) {
        self.unread
            .remove(&(path.components().count(), path.to_owned()));
        self.done
            .insert(path.to_owned(), result.as_ref().ok().copied());
        self.dirty_rows = true;
        match result {
            Ok(end) => {
                self.examined = self.examined.saturating_add(end.examined);
                self.partial |= end.status != ScanStatus::Complete;
            }
            Err(e) => {
                self.errors.insert(path.to_owned(), e);
            }
        }
    }
    pub fn next(&self, list: bool) -> Option<(PathBuf, usize)> {
        self.root.as_ref()?;
        if !self.done.contains_key(Path::new("")) {
            return Some((PathBuf::new(), SCAN_LIMIT));
        }
        if list {
            let remaining = SCAN_LIMIT.saturating_sub(self.examined);
            if remaining == 0 || self.entries.len() >= ENTRY_LIMIT - 1 {
                return None;
            }
            return self.unread.first().map(|(_, p)| (p.clone(), remaining));
        }
        self.tree_next.as_ref().map(|p| (p.clone(), SCAN_LIMIT))
    }
    pub fn limited(&self) -> bool {
        self.partial
            || (!self.unread.is_empty()
                && (self.examined >= SCAN_LIMIT || self.entries.len() >= ENTRY_LIMIT - 1))
    }
    pub fn rows(&mut self) {
        if !self.dirty_rows {
            return;
        }
        self.dirty_rows = false;
        self.tree.clear();
        self.tree_next = None;
        let mut stack = Vec::new();
        if let Some(items) = self.children.get(Path::new("")) {
            stack.extend(items.iter().rev());
        }
        while let Some((_, p)) = stack.pop() {
            self.tree.push(p.clone());
            if self.tree_next.is_none()
                && self.expanded.contains(p)
                && !self.done.contains_key(p)
                && self
                    .entries
                    .get(p)
                    .is_some_and(|e| e.kind == EntryKind::Directory)
            {
                self.tree_next = Some(p.clone());
            }
            if self.expanded.contains(p)
                && let Some(items) = self.children.get(p)
            {
                stack.extend(items.iter().rev());
            }
        }
        let needle = self.filter.to_lowercase();
        self.list = self
            .documents
            .iter()
            .filter(|(_, p)| {
                needle.is_empty() || p.to_string_lossy().to_lowercase().contains(&needle)
            })
            .map(|(_, p)| p.clone())
            .collect();
    }
}

pub(super) fn summary(bytes: &[u8]) -> String {
    let bytes = &bytes[..bytes.len().min(PREFIX_LIMIT)];
    if bytes.contains(&0) {
        return String::new();
    }
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) if bytes.len() == PREFIX_LIMIT && e.error_len().is_none() => {
            std::str::from_utf8(&bytes[..e.valid_up_to()]).unwrap_or_default()
        }
        Err(_) => return String::new(),
    };
    text.split(['\n', '\r'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .take(2)
        .collect::<Vec<_>>()
        .join(" ")
}
