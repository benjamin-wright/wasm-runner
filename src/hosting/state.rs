use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

pub(crate) struct StateDirectory {
    pub path: PathBuf,
    _lock: File,
}

impl StateDirectory {
    pub fn claim(path: &Path) -> Result<Self> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path)?;
        ensure!(
            fs::symlink_metadata(path)?.is_dir(),
            "state path must be a directory, not a symlink"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                fs::metadata(path)?.permissions().mode() & 0o077 == 0,
                "state directory must be private (mode 0700)"
            );
        }
        let path = fs::canonicalize(path)?;
        let lock = path.join("host.lock");
        if let Ok(metadata) = fs::symlink_metadata(&lock) {
            ensure!(metadata.is_file(), "host.lock must be a regular file");
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(lock)?;
        file.try_lock()
            .context("another WR process owns this state directory")?;
        Ok(Self { path, _lock: file })
    }
}
