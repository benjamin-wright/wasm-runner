use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{Context, Result, ensure};
use tokio::{io::AsyncReadExt, process::Command};

use super::{config::Source, ports::RepositoryFiles, release::digest};

pub(crate) struct Repositories {
    root: PathBuf,
    commits: BTreeMap<(String, String), String>,
}

impl Repositories {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            commits: BTreeMap::new(),
        }
    }
}

impl RepositoryFiles for Repositories {
    fn refresh(&mut self) {
        self.commits.clear();
    }

    async fn read(&mut self, source: &Source) -> Result<(String, Vec<u8>)> {
        source.validate()?;
        let key = (source.repo.clone(), source.branch.clone());
        let directory = self.root.join(&digest(source.repo.as_bytes())[7..]);
        let commit = if let Some(commit) = self.commits.get(&key) {
            commit.clone()
        } else {
            std::fs::create_dir_all(&directory)?;
            if !directory.join("HEAD").exists() {
                git(&directory, &["init", "--bare", "--template=", "."]).await?;
            }
            git(
                &directory,
                &[
                    "fetch",
                    "--no-tags",
                    "--no-recurse-submodules",
                    "--no-auto-maintenance",
                    &source.repo,
                    &format!("+refs/heads/{}:refs/wr/current", source.branch),
                ],
            )
            .await?;
            let commit = String::from_utf8(
                git(
                    &directory,
                    &["rev-parse", "--verify", "refs/wr/current^{commit}"],
                )
                .await?,
            )?
            .trim()
            .to_owned();
            self.commits.insert(key, commit.clone());
            commit
        };
        let bytes = git(
            &directory,
            &["cat-file", "blob", &format!("{commit}:{}", source.path)],
        )
        .await?;
        Ok((commit, bytes))
    }
}

async fn git(directory: &Path, arguments: &[&str]) -> Result<Vec<u8>> {
    let mut command = Command::new("git");
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("GIT_") {
            command.env_remove(name);
        }
    }
    command
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "/usr/bin/false")
        .args([
            "-c",
            "credential.helper=",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "protocol.allow=never",
            "-c",
            "protocol.https.allow=always",
            "-c",
            "protocol.http.allow=always",
            "-c",
            "http.lowSpeedLimit=1",
            "-c",
            "http.lowSpeedTime=20",
        ])
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .context("starting Git; install the documented Git prerequisite")?;
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    tokio::time::timeout(Duration::from_secs(60), async {
        let read = |stream: tokio::process::ChildStdout| async move {
            let mut bytes = Vec::new();
            stream.take(1024 * 1024 + 1).read_to_end(&mut bytes).await?;
            ensure!(bytes.len() <= 1024 * 1024, "Git output exceeds 1 MiB");
            Ok::<_, anyhow::Error>(bytes)
        };
        let errors = async {
            let mut bytes = Vec::new();
            stderr.take(65537).read_to_end(&mut bytes).await?;
            ensure!(bytes.len() <= 65536, "Git diagnostics exceed limit");
            Ok::<_, anyhow::Error>(bytes)
        };
        let (output, errors, status) = tokio::try_join!(read(stdout), errors, async {
            Ok::<_, anyhow::Error>(child.wait().await?)
        })?;
        ensure!(
            status.success(),
            "Git {} failed: {}",
            arguments[0],
            String::from_utf8_lossy(&errors)
        );
        Ok(output)
    })
    .await
    .context("Git operation timed out")?
}
