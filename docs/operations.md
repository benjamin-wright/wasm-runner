# Operate a WASM Runner host

This guide covers installing the runner, supervising a production host, and
recovering its local state. Application authors own builds and artifact
publication; see [package and publish an application](guides.md#manage-multiple-applications).
The [command reference](reference.md#start) owns exact CLI and reconciliation
behavior. For local application development, use [Getting started](getting-started.md).

## Install the runner

Use a runner source checkout at the revision you intend to deploy. Follow the
[installation prerequisites](getting-started.md#prerequisites-and-installation),
then run from that checkout:

```sh
make setup
make install
wr --version
wr start --help
```

Record the source revision with the installed binary; the version string alone
may not distinguish builds. Build on the target platform or supply a binary
built for its OS, architecture, and runtime libraries. A production machine
running an already built binary does not need Cargo, Rust, an application source
checkout, or the WebAssembly build target. It does need Git on the service's
`PATH`, HTTPS connectivity to the public repositories and registries, and the
system trust roots needed by Git. WR has no private repository or registry
credential configuration.

Install the binary at a stable absolute path accessible to the service account.
For example, after `make install` on the target Linux machine, with Cargo's
default installation directory:

```sh
sudo install -o root -g root -m 0755 "$HOME/.cargo/bin/wr" /usr/local/bin/wr
/usr/local/bin/wr --version
```

If your build system supplies the binary, substitute that artifact's path.
The service account needs to execute the binary, not modify it. Updating a
developer's Cargo installation does not update this service copy.

## Choose the host configuration

Choose a dedicated unprivileged service account, a persistent state directory,
an application listening address, and an optional loopback admin address. The
Linux example below uses account and group `wr`, state at `/var/lib/wasm-runner`,
application port 8080, and admin port 9001. Create the account and group through
your host provisioning system before starting the service.

There must be **one active WR process per state directory**. The directory must
be private to its owner (mode 0700 on Unix), writable by the service account,
and persist across process restarts. SQL and KV storage are embedded and local;
multiple runner processes do not provide replication or high availability.
Each state directory is also bound to one configuration repository, branch,
and path. Choose a different directory for a different configuration source.

The example listens on loopback for an existing reverse proxy on the same host.
Configure public ingress and TLS through your deployment infrastructure. If
clients or a proxy must connect directly from another machine, choose the
application bind address accordingly. The admin listener only accepts loopback
addresses; it should remain accessible through host-local tooling or your
existing administrative access mechanism.

## Register applications in Git

In your public deployment repository, create `config.yaml`:

```yaml
applications:
  greetings:
    repo: https://example.com/team/greetings.git
    branch: main
    path: application.yaml
```

Replace the example repository URLs with your own. `path` is relative to the
repository root. Each registration key must match the application's manifest
name, and application path prefixes must not overlap. The application manifest
must declare its publication registry and version; its CI pipeline must publish
a complete release as described in the [release workflow](guides.md#manage-multiple-applications).

A change to an application manifest on its registered branch authorizes
deployment. Protect those branches, and the deployment configuration branch,
through your repository's merge controls. There is no separate approval step
inside WR. The host only reads Git and OCI; it needs no merge or push credentials.

Adding a registration enrolls an app; removing one withdraws its routes while
retaining its data. Start with `applications: {}` if intentionally provisioning
an empty host. A missing or invalid file is not a request to remove everything.

## Supervise the process

WR stays in the foreground. Configure your process manager to execute it
directly, preserve the state directory, capture stdout/stderr, and send SIGTERM
on stop. For Linux with systemd, save this example as
`/etc/systemd/system/wasm-runner.service` after substituting the repository URL:

```ini
[Unit]
Description=WASM Runner
Wants=network-online.target
After=network-online.target

[Service]
Type=exec
User=wr
Group=wr
Environment=PATH=/usr/local/bin:/usr/bin:/bin
StateDirectory=wasm-runner
StateDirectoryMode=0700
UMask=0077
ExecStart=/usr/local/bin/wr start --config-repo https://example.com/team/deploy.git --config-branch main --config-path config.yaml --state-dir /var/lib/wasm-runner --listen 127.0.0.1:8080 --admin-listen 127.0.0.1:9001 --poll-interval 30
Restart=on-failure
RestartSec=5s
KillMode=mixed
TimeoutStopSec=infinity

[Install]
WantedBy=multi-user.target
```

`StateDirectory` creates persistent storage owned by the configured service
account with the specified permissions; see [systemd's execution settings](https://www.freedesktop.org/software/systemd/man/latest/systemd.exec.html).
`Type=exec` tracks the foreground process but does not establish application
readiness. `TimeoutStopSec=infinity` lets WR finish draining and syncing without
a timed forced kill; a hung shutdown therefore needs operator attention. If
your environment requires a finite stop deadline, allow for in-flight requests,
deployment preparation, and storage sync. See [systemd's service settings](https://www.freedesktop.org/software/systemd/man/latest/systemd.service.html)
and [shutdown signal behavior](https://github.com/systemd/systemd/blob/main/man/systemd.kill.xml).

On the target host, validate and start the unit:

```sh
sudo systemd-analyze verify /etc/systemd/system/wasm-runner.service
sudo systemctl daemon-reload
sudo systemctl enable --now wasm-runner.service
sudo systemctl status wasm-runner.service
sudo journalctl -u wasm-runner.service -n 100 --no-pager
```

If Git is installed elsewhere, adjust `PATH` in the unit. Keep the unit and host
provisioning in version control. With another process manager, use the same
foreground command and lifecycle requirements; no WR daemon mode or PID file
is needed.

## Check readiness and deployment progress

Run these from the host:

```sh
curl --fail-with-body http://127.0.0.1:9001/live
curl --fail-with-body http://127.0.0.1:9001/ready
curl --fail-with-body http://127.0.0.1:9001/status
```

`/live` checks the listener. `/ready` checks that accepted registrations have
active applications. `/status` reports observations and per-application desired
and active versions, digests, and errors. The [admin reference](reference.md#admin-ui)
defines these responses. Also call a representative application route at its
configured path prefix before considering the deployment verified.

Readiness can stay successful while an update is pending or upstream access is
broken: the old application may still serve traffic. Compare the expected
version with the active version, inspect errors and poll timestamps, and monitor
available disk space. An intentionally empty deployment is also ready. The
browser overview at `http://127.0.0.1:9001/` provides application, storage, and
handler information.

## Upgrade the runner

Build the intended runner revision using the installation workflow and retain
the currently deployed binary and revision record. Plan an interruption: this
host has one active process and no rolling upgrade coordination.

Stop it before replacing the service binary:

```sh
sudo systemctl stop wasm-runner.service
sudo systemctl is-active wasm-runner.service
```

Confirm the old process has exited; `is-active` should report `inactive` and
return a nonzero status. Take a stopped backup as described below. Install the
new binary at the service path, then start and repeat the health and application
checks:

```sh
sudo systemctl start wasm-runner.service
sudo journalctl -u wasm-runner.service -n 100 --no-pager
```

Use `daemon-reload` before starting if the unit changed. Preserve the existing
state directory and config source. Do not assume an older binary can read state
written by a newer version; assess compatibility before reverting a runner
upgrade. Application releases continue to be controlled by their Git manifests,
independently of the runner binary.

## Back up and recover state

Use a stopped backup: stop the service, wait for it to exit, and copy or snapshot
the entire state directory using your backup system while preserving ownership
and permissions. This keeps SQL, KV, accepted configuration, and cached releases
together. Record the runner revision and configuration source with the backup,
then restart and verify the service. A file-by-file copy while WR is writing
does not establish a consistent recovery point.

Restore with the service stopped into an empty destination, using a compatible
runner binary and the same configuration source. Preserve the private directory
permissions and service ownership. Restoring a backup loses writes made after
that recovery point. Once started, WR resumes following Git, so a restored
snapshot is not a permanent pin to its old application versions.

Use the failure reported by `/status` and the service log to choose recovery:

| Observation | Action |
| --- | --- |
| State directory already owned | Find the existing process or supervisor and stop it normally. Never delete `host.lock` to force entry; a crash releases the OS lock automatically. |
| No accepted configuration or unreadable upstream | Check the public URL, branch, relative path, DNS/HTTPS access, and Git availability in the service environment. An existing cached application can continue serving. |
| Desired version differs from active version | Check the per-app error and the publication pipeline. WR retries incomplete releases without another Git change. Keep publication fixes in CI. |
| Invalid component, configuration mismatch, or migration failure | Correct the application release through the normal manifest and publication workflow. A rejected candidate should leave the old generation active. |
| Storage or cached-state failure | Preserve the directory and diagnostics. Restore a complete compatible backup or resolve the reported cause; deleting storage is not a deployment repair. |

For an application rollback, authorize the intended published manifest and
runtime configuration through the normal Git review process. Restoring an old
version does not automatically roll back its database. Follow the
[SQL deployment and rollback rules](reference.md#sql-deployment-and-failures);
a compatible forward release may be required. The [recovery contract](reference.md#start)
also explains the boundary between migration commits and cached release records.
