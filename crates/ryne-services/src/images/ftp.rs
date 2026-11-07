//! Read-only binary FTP, with bounded passive transfers and a shared deadline.
use super::*;
use std::{
    net::{Shutdown, SocketAddr, TcpStream, ToSocketAddrs},
    sync::{mpsc, Mutex},
};
use suppaftp::{types::FileType, FtpError, FtpStream, Mode};

pub(super) fn load(url: &url::Url, control: &CancelControl) -> Result<Vec<u8>, String> {
    if url.query().is_some() {
        return Err("FTP image URLs cannot contain a query".into());
    }
    let username = if url.username().is_empty() {
        "anonymous".into()
    } else {
        field(url.username())?
    };
    let password = field(url.password().unwrap_or(if username == "anonymous" {
        "anonymous@"
    } else {
        ""
    }))?;
    let mut path = url.path().strip_prefix('/').unwrap_or(url.path());
    if path
        .get(path.len().saturating_sub(7)..)
        .is_some_and(|s| s.eq_ignore_ascii_case(";type=i"))
    {
        path = &path[..path.len() - 7];
    }
    let parts = path.split('/').map(field).collect::<Result<Vec<_>, _>>()?;
    let (file, directories) = parts.split_last().ok_or("FTP image URL has no filename")?;
    if file.is_empty() || file.to_ascii_lowercase().contains(";type=") {
        return Err("FTP images require a binary file URL".into());
    }
    let addresses = resolve(url, control)?;
    let mut connected = None;
    for address in addresses {
        if let Ok(socket) = TcpStream::connect_timeout(&address, control.remaining()?) {
            connected = Some(socket);
            break;
        }
    }
    let socket = connected.ok_or("Cannot connect to FTP image server")?;
    let peer = socket.peer_addr().map_err(|e| e.to_string())?;
    let guard = SocketGuard::new(control)?;
    guard.add(&socket, control)?;
    let mut ftp = FtpStream::connect_with_stream(socket).map_err(error)?;
    let sockets = guard.sockets.clone();
    let transfer_control = control.clone();
    ftp = ftp.passive_stream_builder(move |mut address| {
        // PASV addresses commonly contain an unroutable server-side address.
        // Always connect to the same server as the control channel.
        address.set_ip(peer.ip());
        let remaining = transfer_control.remaining().map_err(io_error)?;
        let socket =
            TcpStream::connect_timeout(&address, remaining).map_err(FtpError::ConnectionError)?;
        add_socket(&sockets, &socket, &transfer_control).map_err(io_error)?;
        Ok(socket)
    });
    ftp.login(&username, &password)
        .map_err(|_| "FTP image login failed")?;
    control.checkpoint()?;
    ftp.transfer_type(FileType::Binary).map_err(error)?;
    for directory in directories.iter().filter(|p| !p.is_empty()) {
        control.checkpoint()?;
        ftp.cwd(directory).map_err(error)?;
    }
    ftp.set_mode(Mode::ExtendedPassive);
    let mut download = match ftp.retr_as_stream(file) {
        Ok(stream) => stream,
        Err(FtpError::UnexpectedResponse(response))
            if peer.is_ipv4() && matches!(response.status.code(), 500 | 501 | 502 | 504 | 522) =>
        {
            control.checkpoint()?;
            ftp.set_mode(Mode::Passive);
            ftp.set_passive_nat_workaround(true);
            ftp.retr_as_stream(file).map_err(error)?
        }
        Err(e) => return Err(error(e)),
    };
    let bytes = source::read(&mut download, control);
    if bytes.is_err() {
        guard.shutdown(); // Do not wait for a completion reply after rejecting the payload.
        return bytes;
    }
    download.finish().map_err(error)?;
    control.checkpoint()?;
    bytes
}

fn field(encoded: &str) -> Result<String, String> {
    let decoded = percent_encoding::percent_decode_str(encoded)
        .decode_utf8()
        .map_err(|_| "FTP URL fields must be UTF-8")?;
    if decoded.chars().any(char::is_control) {
        return Err("FTP URL contains control characters".into());
    }
    Ok(decoded.into_owned())
}
fn error(error: FtpError) -> String {
    format!("FTP image transfer failed: {error}")
}
fn io_error(error: impl ToString) -> FtpError {
    FtpError::ConnectionError(std::io::Error::other(error.to_string()))
}

fn resolve(url: &url::Url, control: &CancelControl) -> Result<Vec<SocketAddr>, String> {
    let host = url
        .host_str()
        .ok_or("FTP image URL has no host")?
        .trim_matches(['[', ']'])
        .to_owned();
    let port = url.port().unwrap_or(21);
    let (tx, rx) = mpsc::sync_channel(1);
    // The platform resolver itself cannot be cancelled; the preview queue never waits
    // past its deadline. A timed-out resolver thread exits when the OS call returns.
    std::thread::Builder::new()
        .name("image-ftp-dns".into())
        .spawn(move || {
            let result = (host.as_str(), port).to_socket_addrs().map(|a| a.collect());
            let _ = tx.send(result);
        })
        .map_err(|e| e.to_string())?;
    rx.recv_timeout(control.remaining()?)
        .map_err(|_| "FTP image DNS lookup timed out")?
        .map_err(|e| e.to_string())
}

type Sockets = Arc<Mutex<Vec<TcpStream>>>;
struct SocketGuard {
    sockets: Sockets,
    _done: mpsc::Sender<()>,
    _cancel: ryne_extensions::CancellationRegistration,
}
impl SocketGuard {
    fn new(control: &CancelControl) -> Result<Self, String> {
        let sockets: Sockets = Default::default();
        let on_cancel = sockets.clone();
        let cancel = control.on_cancel(move || shutdown(&on_cancel));
        let on_timeout = sockets.clone();
        let (done, rx) = mpsc::channel();
        let remaining = control.remaining()?;
        std::thread::Builder::new()
            .name("image-ftp-timeout".into())
            .spawn(move || {
                if matches!(
                    rx.recv_timeout(remaining),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    shutdown(&on_timeout);
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            sockets,
            _done: done,
            _cancel: cancel,
        })
    }
    fn add(&self, socket: &TcpStream, control: &CancelControl) -> Result<(), String> {
        add_socket(&self.sockets, socket, control)
    }
    fn shutdown(&self) {
        shutdown(&self.sockets);
    }
}
impl Drop for SocketGuard {
    fn drop(&mut self) {
        self.shutdown();
    }
}
fn shutdown(sockets: &Sockets) {
    for socket in sockets.lock().unwrap_or_else(|e| e.into_inner()).iter() {
        let _ = socket.shutdown(Shutdown::Both);
    }
}
fn add_socket(
    sockets: &Sockets,
    socket: &TcpStream,
    control: &CancelControl,
) -> Result<(), String> {
    let mut sockets = sockets.lock().unwrap_or_else(|e| e.into_inner());
    let remaining = control.remaining()?;
    socket
        .set_read_timeout(Some(remaining))
        .map_err(|e| e.to_string())?;
    socket
        .set_write_timeout(Some(remaining))
        .map_err(|e| e.to_string())?;
    sockets.push(socket.try_clone().map_err(|e| e.to_string())?);
    Ok(())
}
