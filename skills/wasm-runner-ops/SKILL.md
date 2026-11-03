---
name: wasm-runner-ops
description: Install, configure, supervise, upgrade, and recover WASM Runner hosts using wr start and public GitOps deployments. Use for runner service setup, state ownership, health checks, and deployment diagnosis. Excludes application authoring, artifact build/push pipelines, and runner code development.
---

# Operate WASM Runner

Help the user establish or maintain a running WR host using their chosen
deployment infrastructure. Application authoring and artifact publication belong
to the application workflow; this skill consumes published releases.

The canonical procedure is [Operate a WASM Runner host](../../docs/operations.md).
Read the sections relevant to installation, supervision, upgrades, or recovery.
Exact command and failure behavior lives in the linked runner reference.

When this skill has been copied outside the source repository, locate the
matching runner documentation in an available checkout or the documentation
source supplied by the user. The guide is `docs/operations.md` in that source;
its relative link above only works in the repository layout. Use documentation
for the deployed revision and `wr start --help` to check supported options.
Do not assume a package registry, binary download URL, or production source
checkout exists. The production process itself only needs the installed runner,
Git, public upstream access, and persistent storage.

## Establish the deployment inputs

Inspect existing service configuration before proposing a replacement. Resolve
the runner binary/revision, target OS and supervisor, service account, state
directory, application/admin bind addresses, and configuration repository,
branch, and relative path. Ask only for missing choices that affect the task.
Use the documented source installation workflow when a binary is needed.

Treat registered application branches as deployment authorization. The runner
only reads public Git and OCI endpoints; merge controls and push credentials
stay outside the runtime host. Do not add a runtime publication step or private
authentication workflow.

## Preserve the host boundaries

- Run `wr start` in the foreground under the existing supervisor. Let that
  supervisor own restarts and capture logs.
- Keep one active process per persistent state directory. Storage is local,
  with no replication or HA coordination. Keep Unix state private to its service
  owner and preserve the directory across restarts.
- Preserve the configuration source associated with existing state. Use a
  different state directory for a different repository, branch, or config path.
- Keep admin binding on loopback. Fit application ingress and TLS into the
  user's infrastructure rather than treating the admin listener as public ingress.
- Stop gracefully and wait for exit before replacing the runner binary or
  copying/restoring state. Never delete the lock file to bypass another owner.

Use the guide's systemd example only for a systemd host. Adapt its concrete
paths, account, URL, and ports to the deployment. Keep service configuration
reviewable and reproducible through the user's provisioning system.

## Verify and diagnose

Check `/live`, `/ready`, `/status`, and a representative application route.
Readiness alone does not prove the requested version deployed: compare active
and desired versions and inspect per-app errors and poll timestamps. An empty
accepted configuration is ready, and an older active release can remain ready
through a failed update or upstream outage.

Diagnose from the observed failure and retain working state. Missing artifacts
retry automatically; app removal retains data. Route publication or manifest
repairs back to the normal application workflow. Do not repeatedly restart the
host or clear caches/databases to make an unexplained failure disappear.
Application rollback must respect migration history; a runner downgrade must
respect storage compatibility. Use the guide's stopped backup and recovery
procedure when restoration is actually required.

Report the configured service, binary revision, state location, observed active
versions, checks performed, and any remaining blocker. Distinguish prepared
configuration from a service that was actually started and verified.
