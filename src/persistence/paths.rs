use anyhow::{Result, ensure};
use std::{fs, path::Path};

pub(super) fn prepare_directory(root: &Path) -> Result<()> {
    // Validate each existing ancestor: create_dir_all otherwise follows symlinks.
    for ancestor in root.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => ensure!(
                metadata.is_dir(),
                "storage path must contain only directories: {}",
                ancestor.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        ensure!(
            fs::metadata(root)?.permissions().mode() & 0o077 == 0,
            "storage directory must be private (mode 0700)"
        );
    }
    validate_managed_paths(root)
}

fn validate_managed_paths(root: &Path) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(
            kind.is_file() || kind.is_dir(),
            "invalid managed storage path: {}",
            entry.path().display()
        );
        if kind.is_dir() {
            validate_managed_paths(&entry.path())?;
        }
    }
    Ok(())
}
