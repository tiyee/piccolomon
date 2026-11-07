use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    thread,
    time::Instant,
};

// Local peers exercise the real protocol stack without public FTP infrastructure.
fn ftp_server(
    bytes: Vec<u8>,
    epsv: bool,
    completed: bool,
    stall: bool,
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        socket
            .set_write_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        if stall {
            // A trickle must not renew the request's global deadline.
            for _ in 0..100 {
                if socket.write_all(b"2").is_err() {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
            return Vec::new();
        }
        socket
            .write_all(b"220-Local image fixture\r\n220 Ready\r\n")
            .unwrap();
        let mut reader = BufReader::new(socket.try_clone().unwrap());
        let mut data_listener = None;
        let mut commands = Vec::new();
        loop {
            let mut command = String::new();
            if reader.read_line(&mut command).unwrap_or(0) == 0 {
                break;
            }
            commands.push(command.trim_end().to_owned());
            let (verb, _) = command
                .trim_end()
                .split_once(' ')
                .unwrap_or((command.trim_end(), ""));
            let reply = match verb {
                "USER" => "331 Password required\r\n".into(),
                "PASS" => "230 Logged in\r\n".into(),
                "TYPE" => "200 Binary\r\n".into(),
                "CWD" => "250 Directory changed\r\n".into(),
                "EPSV" if !epsv => "502 EPSV unsupported\r\n".into(),
                "EPSV" | "PASV" => {
                    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
                    let port = listener.local_addr().unwrap().port();
                    data_listener = Some(listener);
                    if verb == "EPSV" {
                        format!("229 Entering Extended Passive Mode (|||{port}|)\r\n")
                    } else {
                        // Advertised IP deliberately differs; use the control peer.
                        format!(
                            "227 Entering Passive Mode (10,0,0,1,{},{})\r\n",
                            port / 256,
                            port % 256
                        )
                    }
                }
                "RETR" => {
                    socket
                        .write_all(b"150 Opening binary connection\r\n")
                        .unwrap();
                    let (mut data, _) = data_listener.take().unwrap().accept().unwrap();
                    data.set_write_timeout(Some(Duration::from_secs(5)))
                        .unwrap();
                    let _ = data.write_all(&bytes);
                    drop(data);
                    let _ = socket.write_all(if completed {
                        b"226 Transfer complete\r\n"
                    } else {
                        b"426 Transfer aborted\r\n"
                    });
                    break;
                }
                _ => panic!("unexpected FTP command: {verb}"),
            };
            socket.write_all(reply.as_bytes()).unwrap();
        }
        commands
    });
    (format!("ftp://{address}"), worker)
}
#[test]
fn ftp_downloads_binary_raster_and_svg_with_epsv_and_pasv() {
    for epsv in [true, false] {
        let (root, server) = ftp_server(
            if epsv {
                png(2, 3)
            } else {
                SVG.as_bytes().to_vec()
            },
            epsv,
            true,
            false,
        );
        let url = format!("{root}/%E5%9B%BE%E7%89%87/a%20%23.png;type=i");
        let result = ImageRenderer.render(&url, CancelControl::default());
        let commands = server.join().unwrap();
        assert!(result.is_ok(), "{result:?}");
        assert!(commands.contains(&"USER anonymous".into()));
        assert!(commands.contains(&"TYPE I".into()));
        assert!(commands.contains(&"CWD 图片".into()));
        assert!(commands.contains(&"RETR a #.png".into()));
        assert_eq!(commands.contains(&"PASV".into()), !epsv);
    }
    for (credentials, expected) in [("user%20name:p%40ss", "PASS p@ss"), ("user%20name", "PASS")] {
        let (root, server) = ftp_server(png(1, 1), true, true, false);
        let address = root.replacen("ftp://", &format!("ftp://{credentials}@"), 1) + "/a.png";
        assert!(ImageRenderer
            .render(&address, CancelControl::default())
            .is_ok());
        let commands = server.join().unwrap();
        assert!(commands.contains(&"USER user name".into()));
        assert!(commands.contains(&expected.into()));
    }
}
#[test]
fn ftp_rejects_truncated_oversized_and_stalled_transfers() {
    for (bytes, completed) in [(png(1, 1), false), (vec![0; MAX_DOWNLOAD + 1], true)] {
        let (root, server) = ftp_server(bytes, true, completed, false);
        assert!(ImageRenderer
            .render(&(root + "/a.png"), CancelControl::default())
            .is_err());
        server.join().unwrap();
    }
    let (root, server) = ftp_server(Vec::new(), true, true, true);
    let start = Instant::now();
    let result = ImageRenderer.render(
        &(root + "/a.png"),
        CancelControl::with_timeout(Duration::from_millis(200)),
    );
    assert!(result.is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    server.join().unwrap();
}
#[test]
fn ftp_cancellation_interrupts_blocking_handshake() {
    let (root, server) = ftp_server(Vec::new(), true, true, true);
    let control = CancelControl::with_timeout(Duration::from_secs(5));
    let cancel = control.clone();
    let cancel_worker = thread::spawn(move || {
        thread::sleep(Duration::from_millis(150));
        cancel.cancel();
    });
    let start = Instant::now();
    assert!(ImageRenderer.render(&(root + "/a.png"), control).is_err());
    assert!(start.elapsed() < Duration::from_secs(1));
    cancel_worker.join().unwrap();
    server.join().unwrap();
}
#[test]
fn http_loads_svg_by_content_without_a_filename_extension() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(socket.try_clone().unwrap());
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
        }
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{SVG}", SVG.len()).unwrap();
    });
    let result = ImageRenderer.render(&format!("http://{address}/image"), CancelControl::default());
    worker.join().unwrap();
    let image = result.unwrap();
    assert_eq!((image.width, image.height), (20., 10.));
}
