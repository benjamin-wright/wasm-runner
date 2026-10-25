use std::{io, net::SocketAddr, path::Path};

pub(crate) fn run(manifest: &Path, listen: SocketAddr) -> io::Result<()> {
    // Prepare the entire application before binding or announcing readiness.
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        format!(
            "run is not implemented yet (manifest: {}, listen: {listen})",
            manifest.display()
        ),
    ))
}
