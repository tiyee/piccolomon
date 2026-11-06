use ryne_extensions::directory::*;
use std::{
    ffi::OsStr,
    fs,
    io::{self, Read},
    path::Path,
};

pub struct LocalDirectory;
fn document_name(path: &Path) -> bool {
    match path.extension() {
        Some(ext) => ["md", "markdown", "mdown", "mkd", "txt"]
            .iter()
            .any(|x| ext.eq_ignore_ascii_case(x)),
        None => path
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("readme") || n.eq_ignore_ascii_case("license")),
    }
}
fn linked(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.is_symlink()
    }
}
impl DirectoryBrowser for LocalDirectory {
    fn enumerate(
        &self,
        request: ScanRequest<'_>,
        cancelled: &dyn Fn() -> bool,
        emit: &mut dyn FnMut(Vec<DirectoryEntry>) -> bool,
    ) -> io::Result<ScanEnd> {
        // A cached child may have been replaced with a junction or symlink.
        // The root was selected explicitly and may itself contain links.
        let mut path = request.root.to_owned();
        for part in request.relative.components() {
            let std::path::Component::Normal(name) = part else {
                return Err(io::Error::other("invalid directory component"));
            };
            path.push(name);
            if linked(&fs::symlink_metadata(&path)?) {
                return Err(io::Error::other("directory link"));
            }
        }
        let mut end = ScanEnd::default();
        let mut batch = Vec::with_capacity(BATCH);
        for item in fs::read_dir(path)? {
            if cancelled() {
                end.status = ScanStatus::Cancelled;
                return Ok(end);
            }
            if end.examined >= request.limit {
                end.status = ScanStatus::Truncated;
                break;
            }
            end.examined += 1;
            let item = item?;
            let name = item.file_name();
            let path = item.path();
            let current = Some(path.as_path()) == request.current;
            if !current
                && (name.as_encoded_bytes().starts_with(b".")
                    || name == OsStr::new("target")
                    || name == OsStr::new("node_modules"))
            {
                continue;
            }
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            let link = linked(&meta);
            let meta = if link {
                let Ok(target) = fs::metadata(&path) else {
                    continue;
                };
                target
            } else {
                meta
            };
            let kind = if meta.is_dir() {
                if link {
                    EntryKind::DirectoryLink
                } else {
                    EntryKind::Directory
                }
            } else if meta.is_file() && (current || document_name(&path)) {
                EntryKind::Document
            } else {
                continue;
            };
            batch.push(DirectoryEntry {
                name,
                kind,
                modified: meta.modified().ok(),
                bytes: Some(meta.len()),
            });
            if batch.len() == BATCH
                && (cancelled() || !emit(std::mem::replace(&mut batch, Vec::with_capacity(BATCH))))
            {
                end.status = ScanStatus::Cancelled;
                return Ok(end);
            }
        }
        if cancelled() || (!batch.is_empty() && !emit(batch)) {
            end.status = ScanStatus::Cancelled;
        }
        Ok(end)
    }
    fn prefix(&self, path: &Path) -> io::Result<Vec<u8>> {
        if !fs::metadata(path)?.is_file() {
            return Err(io::Error::other("expected a regular file"));
        }
        let mut options = fs::OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options.open(path)?;
        if !file.metadata()?.is_file() {
            return Err(io::Error::other("expected a regular file"));
        }
        let mut bytes = Vec::new();
        file.take(PREFIX_LIMIT as u64).read_to_end(&mut bytes)?;
        Ok(bytes)
    }
    fn metadata(&self, path: &Path) -> io::Result<DirectoryEntry> {
        let meta = fs::metadata(path)?;
        if !meta.is_file() {
            return Err(io::Error::other("expected a regular file"));
        }
        Ok(DirectoryEntry {
            name: path
                .file_name()
                .ok_or_else(|| io::Error::other("missing file name"))?
                .to_owned(),
            kind: EntryKind::Document,
            modified: meta.modified().ok(),
            bytes: Some(meta.len()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[test]
    fn supported_documents_bounded_reads_and_filtered_entries_count() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in [
            "doc.MARKDOWN",
            "README",
            "License",
            "article.mkd",
            "notes.txt",
            "current.weird",
            "ignore.bin",
            ".secret.md",
        ] {
            fs::write(root.join(name), "text").unwrap();
        }
        for name in ["sub", "target", "node_modules"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(root, root.join("loop")).unwrap();
            std::os::unix::fs::symlink(root.join("notes.txt"), root.join("link.md")).unwrap();
        }
        // APFS requires valid UTF-8 names. Other Unix filesystems can exercise native bytes on disk.
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            use std::os::unix::ffi::OsStringExt;
            fs::write(
                root.join(std::ffi::OsString::from_vec(b"bad\xff.md".to_vec())),
                "ok",
            )
            .unwrap();
        }
        let mut entries = BTreeMap::new();
        let result = LocalDirectory
            .enumerate(
                ScanRequest {
                    root,
                    relative: Path::new(""),
                    current: Some(&root.join("current.weird")),
                    limit: SCAN_LIMIT,
                },
                &|| false,
                &mut |batch| {
                    assert!(batch.len() <= BATCH);
                    entries.extend(batch.into_iter().map(|e| (e.name, e.kind)));
                    true
                },
            )
            .unwrap();
        assert_eq!(result.status, ScanStatus::Complete);
        assert_eq!(result.examined, fs::read_dir(root).unwrap().count());
        for name in [
            "doc.MARKDOWN",
            "README",
            "License",
            "article.mkd",
            "notes.txt",
            "current.weird",
        ] {
            assert_eq!(entries.get(OsStr::new(name)), Some(&EntryKind::Document));
        }
        for name in ["target", "node_modules", "ignore.bin", ".secret.md"] {
            assert!(!entries.contains_key(OsStr::new(name)));
        }
        #[cfg(unix)]
        {
            assert_eq!(
                entries.get(OsStr::new("loop")),
                Some(&EntryKind::DirectoryLink)
            );
            assert_eq!(
                entries.get(OsStr::new("link.md")),
                Some(&EntryKind::Document)
            );
        }
        #[cfg(all(unix, not(target_os = "macos")))]
        {
            use std::os::unix::ffi::OsStrExt;
            let native = OsStr::from_bytes(b"bad\xff.md");
            assert!(entries.contains_key(native));
            assert_eq!(LocalDirectory.prefix(&root.join(native)).unwrap(), b"ok");
        }
        let limited = LocalDirectory
            .enumerate(
                ScanRequest {
                    root,
                    relative: Path::new(""),
                    current: None,
                    limit: 3,
                },
                &|| false,
                &mut |_| true,
            )
            .unwrap();
        assert_eq!(limited.status, ScanStatus::Truncated);
        assert_eq!(limited.examined, 3);
        assert_eq!(
            LocalDirectory
                .enumerate(
                    ScanRequest {
                        root,
                        relative: Path::new(""),
                        current: None,
                        limit: 3
                    },
                    &|| true,
                    &mut |_| panic!("cancelled")
                )
                .unwrap()
                .examined,
            0
        );
        fs::write(root.join("long.md"), vec![b'x'; PREFIX_LIMIT + 2]).unwrap();
        assert_eq!(
            LocalDirectory.prefix(&root.join("long.md")).unwrap().len(),
            PREFIX_LIMIT
        );
        assert!(LocalDirectory.prefix(root).is_err());
    }
}
