use super::protocol::{MAX_MESSAGE, Reply, Request};
use anyhow::{Context, Result, bail, ensure};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::Path,
    time::Duration,
};

pub(crate) fn request(socket: &Path, request: Request) -> Result<Reply> {
    let bytes = serde_json::to_vec(&request)?;
    ensure!(
        bytes.len() <= MAX_MESSAGE,
        "control message exceeds 128 MiB"
    );
    let mut stream = UnixStream::connect(socket).with_context(|| {
        format!(
            "connecting to daemon at {}; run `wr daemon start` first",
            socket.display()
        )
    })?;
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    stream.write_all(&(bytes.len() as u32).to_be_bytes())?;
    stream.write_all(&bytes)?;
    let mut size = [0; 4];
    stream.read_exact(&mut size)?;
    let size = u32::from_be_bytes(size) as usize;
    ensure!(size <= MAX_MESSAGE, "control response exceeds 128 MiB");
    let mut bytes = vec![0; size];
    stream.read_exact(&mut bytes)?;
    let reply = serde_json::from_slice(&bytes).context("invalid control response")?;
    if let Reply::Error { message } = reply {
        bail!("{message}");
    }
    Ok(reply)
}
