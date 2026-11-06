//! Explorer starts the executable again for a file association. A primary
//! instance publishes a local-only pipe in the user's configuration directory.
//! The session-specific lease is released by Windows even after a crash.
use std::{
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    os::windows::{
        ffi::{OsStrExt, OsStringExt},
        io::{AsRawHandle, FromRawHandle},
    },
    path::{Path, PathBuf},
    ptr,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE},
    Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND},
    System::{
        Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_REJECT_REMOTE_CLIENTS,
        },
        RemoteDesktop::ProcessIdToSessionId,
    },
};
const MAX_UNITS: usize = 32767;
fn pipe_name(id: uuid::Uuid) -> String {
    format!(r"\\.\pipe\Ryne-{id}")
}
fn serve(mut pipe: File, sender: async_channel::Sender<PathBuf>, _lease: File) {
    loop {
        // Synchronous handle, valid until this function exits. A client can
        // connect between creation and ConnectNamedPipe.
        if unsafe { ConnectNamedPipe(pipe.as_raw_handle(), ptr::null_mut()) } == 0
            && io::Error::last_os_error().raw_os_error() != Some(ERROR_PIPE_CONNECTED as i32)
        {
            break;
        }
        let mut length = [0; 4];
        let path = (|| -> Option<PathBuf> {
            pipe.read_exact(&mut length).ok()?;
            let n = u32::from_le_bytes(length) as usize;
            if !(1..=MAX_UNITS).contains(&n) {
                return None;
            }
            let mut bytes = vec![0; n * 2];
            pipe.read_exact(&mut bytes).ok()?;
            let wide: Vec<_> = bytes
                .chunks_exact(2)
                .map(|p| u16::from_le_bytes([p[0], p[1]]))
                .collect();
            let path = PathBuf::from(OsString::from_wide(&wide));
            (path.is_absolute() && !wide.contains(&0)).then_some(path)
        })();
        unsafe {
            DisconnectNamedPipe(pipe.as_raw_handle());
        }
        if let Some(path) = path
            && sender.send_blocking(path).is_err()
        {
            break;
        }
    }
}
/// True when this invocation delivered its file to the existing instance.
pub(super) fn start(
    config: &Path,
    path: Option<&Path>,
    sender: async_channel::Sender<PathBuf>,
) -> io::Result<bool> {
    let mut session = 0;
    if unsafe { ProcessIdToSessionId(std::process::id(), &mut session) } == 0 {
        return Err(io::Error::last_os_error());
    }
    fs::create_dir_all(config)?;
    let lease = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(config.join(format!("activation-{session}.lock")))?;
    let endpoint = config.join(format!("activation-{session}.id"));
    match lease.try_lock() {
        Ok(()) => {
            let id = uuid::Uuid::new_v4();
            let name = pipe_name(id);
            let wide: Vec<_> = OsStr::new(&name).encode_wide().chain(Some(0)).collect();
            // Default DACL from this user's access token; disallow remote pipe
            // clients and fail if anyone has pre-created this instance.
            let handle = unsafe {
                CreateNamedPipeW(
                    wide.as_ptr(),
                    PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                    PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    0,
                    65536,
                    0,
                    ptr::null(),
                )
            };
            if handle == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let pipe = unsafe { File::from_raw_handle(handle) };
            let tmp = endpoint.with_extension("next");
            fs::write(&tmp, id.to_string())?;
            fs::rename(tmp, endpoint)?;
            std::thread::Builder::new()
                .name("ryne-file-activation".into())
                .spawn(move || serve(pipe, sender, lease))?;
            Ok(false)
        }
        Err(fs::TryLockError::WouldBlock) => {
            let Some(path) = path else { return Ok(false) };
            let units: Vec<_> = path.as_os_str().encode_wide().collect();
            if units.len() > MAX_UNITS {
                return Err(io::Error::other("file path is too long"));
            }
            let mut data = (units.len() as u32).to_le_bytes().to_vec();
            data.extend(units.into_iter().flat_map(u16::to_le_bytes));
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                if let Some(mut pipe) = fs::read_to_string(&endpoint)
                    .ok()
                    .and_then(|s| uuid::Uuid::parse_str(&s).ok())
                    .and_then(|id| OpenOptions::new().write(true).open(pipe_name(id)).ok())
                {
                    pipe.write_all(&data)?;
                    return Ok(true);
                }
                if Instant::now() >= deadline {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "existing Ryne window did not accept the file",
                    ));
                }
                std::thread::sleep(Duration::from_millis(25));
            }
        }
        Err(fs::TryLockError::Error(e)) => Err(e),
    }
}
