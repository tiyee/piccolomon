//! The numbered lock files are permanent rendezvous points. An OS lock releases
//! on process exit, so another instance can recover the last complete snapshot.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

fn private_file() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}

fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

pub(super) struct Slot {
    path: PathBuf,
    _lease: File,
}

impl Slot {
    fn claim(dir: &Path, id: u64) -> io::Result<Option<Self>> {
        let lease = private_file()
            .create(true)
            .truncate(false)
            .open(dir.join(format!("{id}.lock")))?;
        match lease.try_lock() {
            Ok(()) => Ok(Some(Self {
                path: dir.join(format!("{id}.md")),
                _lease: lease,
            })),
            Err(fs::TryLockError::WouldBlock) => Ok(None),
            Err(fs::TryLockError::Error(e)) => Err(e),
        }
    }

    fn allocate(dir: &Path) -> io::Result<Self> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(dir)?;
        for id in 0..u64::MAX {
            if let Some(slot) = Self::claim(dir, id)?
                && !slot.path.try_exists()?
            {
                return Ok(slot);
            }
        }
        Err(io::Error::other("no available draft slot"))
    }

    pub(super) fn read(dir: &Path, id: u64) -> io::Result<Option<(Self, String)>> {
        let Some(slot) = Self::claim(dir, id)? else {
            return Ok(None);
        };
        match fs::read_to_string(&slot.path) {
            Ok(text) => Ok(Some((slot, text))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn write(&self, text: &str) -> io::Result<()> {
        let temp = self.path.with_extension("tmp");
        remove_if_present(&temp)?;
        let mut file = private_file().create_new(true).open(&temp)?;
        let result = file
            .write_all(text.as_bytes())
            .and_then(|()| file.sync_all());
        drop(file);
        let result = result.and_then(|()| fs::rename(&temp, &self.path));
        if result.is_err() {
            let _ = remove_if_present(&temp);
        }
        result
    }
    pub(super) fn discard(&self) -> io::Result<()> {
        remove_if_present(&self.path.with_extension("tmp"))?;
        remove_if_present(&self.path)
    }
}

/// These files are owned by the recovery protocol. Saving a document over its
/// own slot and then retiring that slot would delete the just-saved file.
pub(super) fn reserved_path(directory: &Path, path: &Path) -> bool {
    let Ok(root) = directory.canonicalize() else {
        return false;
    };
    let resolved = path
        .canonicalize()
        .ok()
        .or_else(|| Some(path.parent()?.canonicalize().ok()?.join(path.file_name()?)));
    resolved.is_some_and(|p| {
        p.parent() == Some(root.as_path())
            && p.file_stem()
                .and_then(|s| s.to_str())
                .is_some_and(|s| s.parse::<u64>().is_ok())
            && p.extension().and_then(|s| s.to_str()).is_some_and(|ext| {
                ["md", "tmp", "lock"]
                    .iter()
                    .any(|name| ext.eq_ignore_ascii_case(name))
            })
    })
}

pub(super) fn candidates(dir: &Path) -> io::Result<Vec<u64>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };
    let mut ids = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "md")
            && let Some(id) = path
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<u64>().ok())
            && entry.file_type()?.is_file()
        {
            ids.push(id);
        }
    }
    ids.sort_unstable();
    Ok(ids)
}

enum State {
    New,
    Held(Slot),
    Retired,
}

/// Only the background writer and an explicit retirement take this mutex.
/// Even a previously dispatched write cannot recreate a discarded draft.
pub(super) struct Draft {
    state: Mutex<State>,
}
impl Draft {
    pub(super) fn new(slot: Option<Slot>) -> Self {
        Self {
            state: Mutex::new(slot.map_or(State::New, State::Held)),
        }
    }
    pub(super) fn write(&self, directory: &Path, text: &str) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("draft writer unavailable"))?;
        if matches!(*state, State::New) {
            *state = State::Held(Slot::allocate(directory)?);
        }
        if let State::Held(slot) = &*state {
            slot.write(text)?;
        }
        Ok(())
    }
    pub(super) fn retire(&self) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("draft writer unavailable"))?;
        if let State::Held(slot) = &*state {
            slot.discard()?;
        }
        *state = State::Retired;
        Ok(())
    }
}
