use super::*;
use ryne_extensions::image::is_data_url;
use std::{fs::File, io::Read};

pub(super) fn load(source: &str, control: &CancelControl) -> Result<Vec<u8>, String> {
    if is_data_url(source) {
        return data(source, control);
    }
    let url = url::Url::parse(source).map_err(|_| "Invalid image URL")?;
    match url.scheme() {
        "http" | "https" => http(&url, control),
        "ftp" => ftp::load(&url, control),
        "file" => {
            let path = url.to_file_path().map_err(|_| "Invalid local file URL")?;
            // Preflight also avoids opening a device or blocking on a named pipe.
            check_file(&std::fs::metadata(&path).map_err(|e| e.to_string())?)?;
            let file = File::open(path).map_err(|e| e.to_string())?;
            check_file(&file.metadata().map_err(|e| e.to_string())?)?;
            read(file, control)
        }
        _ => Err("Images require an HTTP(S), FTP, file or data URL".into()),
    }
}

fn check_file(metadata: &std::fs::Metadata) -> Result<(), String> {
    if !metadata.is_file() || metadata.len() > MAX_DOWNLOAD as u64 {
        Err("Image must be a regular file no larger than 8 MiB".into())
    } else {
        Ok(())
    }
}

fn http(url: &url::Url, control: &CancelControl) -> Result<Vec<u8>, String> {
    if url.host_str().is_none() || !url.username().is_empty() || url.password().is_some() {
        return Err("HTTP(S) images require a URL without credentials".into());
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(control.remaining()?))
        .max_redirects(5)
        .build()
        .into();
    let mut response = agent
        .get(url.as_str())
        .header("User-Agent", "Ryne")
        .header(
            "Accept",
            "image/svg+xml, image/png, image/jpeg, image/gif, image/webp",
        )
        .call()
        .map_err(|e| e.to_string())?;
    read(response.body_mut().as_reader(), control)
}

pub(super) fn read(mut input: impl Read, control: &CancelControl) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 32 * 1024];
    loop {
        control.checkpoint()?;
        let count = input.read(&mut chunk).map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(bytes);
        }
        if bytes.len() + count > MAX_DOWNLOAD {
            return Err("Image exceeds 8 MiB".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}

fn data(source: &str, control: &CancelControl) -> Result<Vec<u8>, String> {
    let url = data_url::DataUrl::process(source).map_err(|_| "Invalid data URL")?;
    let mut bytes = Vec::new();
    let mut next_check = 0;
    url.decode(|chunk| {
        if bytes.len() >= next_check {
            control.checkpoint()?;
            next_check = bytes.len() + 32 * 1024;
        }
        if bytes.len() + chunk.len() > MAX_DOWNLOAD {
            return Err("Decoded image exceeds 8 MiB");
        }
        bytes.extend_from_slice(chunk);
        Ok(())
    })
    .map_err(|e| format!("Invalid image data: {e:?}"))?;
    control.checkpoint()?;
    Ok(bytes)
}
