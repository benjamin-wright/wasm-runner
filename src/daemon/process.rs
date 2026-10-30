use anyhow::{Context, Result, bail, ensure};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader},
    net::SocketAddr,
    os::unix::{
        fs::{DirBuilderExt, FileTypeExt, OpenOptionsExt, PermissionsExt},
        process::CommandExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

pub(crate) struct Paths {
    pub directory: PathBuf,
    pub socket: PathBuf,
    pub log: PathBuf,
}

impl Paths {
    pub fn new(directory: Option<PathBuf>) -> Result<Self> {
        let directory =
            match directory.or_else(|| std::env::var_os("WR_STATE_DIR").map(PathBuf::from)) {
                Some(path) => path,
                None => PathBuf::from(
                    std::env::var_os("HOME").context("HOME is unset; supply --state-dir")?,
                )
                .join(".wr/daemon"),
            };
        let directory = std::path::absolute(directory)?;
        Ok(Self {
            socket: directory.join("control.sock"),
            log: directory.join("daemon.log"),
            directory,
        })
    }

    pub fn prepare(&self) -> Result<()> {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&self.directory)?;
        let metadata = fs::symlink_metadata(&self.directory)?;
        ensure!(
            metadata.is_dir() && metadata.permissions().mode() & 0o077 == 0,
            "daemon state directory must be a private directory (mode 0700): {}",
            self.directory.display()
        );
        Ok(())
    }

    fn lock_file(&self) -> Result<File> {
        let path = self.directory.join("daemon.lock");
        reject_symlink(&path)?;
        Ok(OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?)
    }

    pub fn claim(&self) -> Result<Guard> {
        self.prepare()?;
        let file = self.lock_file()?;
        file.try_lock()
            .context("a daemon is already running or starting in this state directory")?;
        match fs::symlink_metadata(&self.socket) {
            Ok(metadata) => {
                ensure!(
                    metadata.file_type().is_socket(),
                    "control socket path is occupied by a non-socket file"
                );
                fs::remove_file(&self.socket)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(Guard {
            _lock: file,
            socket: self.socket.clone(),
        })
    }

    pub fn wait_stopped(&self) -> Result<()> {
        let file = self.lock_file()?;
        let deadline = Instant::now() + Duration::from_secs(120);
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(()),
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(error) => return Err(error.into()),
            }
            ensure!(
                Instant::now() < deadline,
                "timed out waiting for daemon shutdown"
            );
            std::thread::sleep(Duration::from_millis(25));
        }
    }
}

pub(crate) struct Guard {
    _lock: File,
    socket: PathBuf,
}

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.socket);
    }
}

fn reject_symlink(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => ensure!(
            metadata.is_file(),
            "expected a regular file: {}",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

pub(crate) fn start(paths: &Paths, listen: SocketAddr) -> Result<String> {
    paths.prepare()?;
    reject_symlink(&paths.log)?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(&paths.log)?;
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--state-dir")
        .arg(&paths.directory)
        .args(["daemon", "serve", "--listen", &listen.to_string()])
        .current_dir(&paths.directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(log)
        .process_group(0)
        .spawn()
        .context("starting daemon process")?;
    let stdout = child.stdout.take().expect("piped stdout");
    let (sent, received) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        let result = BufReader::new(stdout).read_line(&mut line).map(|_| line);
        let _ = sent.send(result);
    });
    let ready = received.recv_timeout(Duration::from_secs(30));
    match ready {
        Ok(Ok(line)) if line.starts_with("Listening on http://") => {
            let _ = reader.join();
            Ok(line.trim().to_owned())
        }
        result => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            bail!(
                "daemon did not become ready ({result:?}); see {}",
                paths.log.display()
            )
        }
    }
}
