use std::{
    collections::BTreeMap,
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, ensure};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    MAX_SQL_BYTES, SqlStore,
    connection::{Restricted, Transaction},
};

const MAX_MIGRATIONS: usize = 1000;
const MAX_MIGRATION_BYTES: usize = 8 * 1024 * 1024;
const MIGRATION_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct SqlConfig {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration_target: Option<u64>,
}

impl SqlConfig {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.enabled || self.migration_target.is_none(),
            "sql.migration-target requires sql.enabled"
        );
        ensure!(
            self.migration_target
                .is_none_or(|target| target <= i64::MAX as u64),
            "migration target is too large"
        );
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Migration {
    pub id: u64,
    pub name: String,
    pub apply: String,
    pub rollback: Option<String>,
}

pub fn validate_migrations(migrations: &[Migration], target: Option<u64>) -> Result<()> {
    ensure!(migrations.len() <= MAX_MIGRATIONS, "too many migrations");
    let mut previous = 0;
    let mut bytes = 0;
    for migration in migrations {
        ensure!(
            migration.id > previous && migration.id <= i64::MAX as u64,
            "migration IDs must be unique, positive, and increasing"
        );
        ensure!(
            crate::identifier::is_valid(&migration.name),
            "invalid migration name"
        );
        ensure!(
            !migration.apply.trim().is_empty(),
            "migration {} has no apply SQL",
            migration.id
        );
        ensure!(
            migration.apply.len() <= MAX_SQL_BYTES
                && migration
                    .rollback
                    .as_ref()
                    .is_none_or(|sql| !sql.trim().is_empty() && sql.len() <= MAX_SQL_BYTES),
            "migration {} has empty or oversized SQL",
            migration.id
        );
        bytes += migration.apply.len() + migration.rollback.as_ref().map_or(0, String::len);
        previous = migration.id;
    }
    ensure!(
        bytes <= MAX_MIGRATION_BYTES,
        "migration bundle exceeds 8 MiB"
    );
    ensure!(
        target.is_none_or(|target| target == 0 || migrations.iter().any(|m| m.id == target)),
        "unknown migration target"
    );
    Ok(())
}

struct History {
    id: u64,
    name: String,
    apply_hash: Vec<u8>,
    rollback_hash: Option<Vec<u8>>,
    applied: bool,
}

fn hash(sql: &str) -> Vec<u8> {
    Sha256::digest(sql.as_bytes()).to_vec()
}

fn history(connection: &Connection) -> Result<Vec<History>> {
    let mut statement = connection.prepare(
        "SELECT id, name, apply_hash, rollback_hash, applied FROM _wr_migrations ORDER BY id",
    )?;
    Ok(statement
        .query_map([], |row| {
            Ok(History {
                id: row.get::<_, i64>(0)? as u64,
                name: row.get(1)?,
                apply_hash: row.get(2)?,
                rollback_hash: row.get(3)?,
                applied: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?)
}

fn plan<'a>(
    migrations: &'a [Migration],
    history: &[History],
    target: Option<u64>,
) -> Result<(u64, Vec<(&'a Migration, bool)>)> {
    let supplied: BTreeMap<_, _> = migrations.iter().map(|m| (m.id, m)).collect();
    let recorded: BTreeMap<_, _> = history.iter().map(|h| (h.id, h)).collect();
    let current = history
        .iter()
        .filter(|h| h.applied)
        .map(|h| h.id)
        .max()
        .unwrap_or(0);
    let high_water = history.last().map_or(0, |h| h.id);
    for old in history {
        let migration = supplied
            .get(&old.id)
            .with_context(|| format!("missing recorded migration {}", old.id))?;
        ensure!(
            old.name == migration.name
                && old.apply_hash == hash(&migration.apply)
                && old.rollback_hash == migration.rollback.as_deref().map(hash),
            "migration {} differs from recorded history",
            old.id
        );
        ensure!(
            old.applied == (old.id <= current),
            "inconsistent migration history"
        );
    }
    for migration in migrations {
        ensure!(
            migration.id > high_water || recorded.contains_key(&migration.id),
            "cannot insert migration {} into recorded history",
            migration.id
        );
    }
    let target = target.unwrap_or_else(|| migrations.last().map_or(0, |m| m.id).max(current));
    let changes = if target < current {
        let mut changes = Vec::new();
        for old in history.iter().rev().filter(|h| h.applied && h.id > target) {
            let migration = supplied[&old.id];
            ensure!(
                migration.rollback.is_some(),
                "migration {} has no rollback SQL",
                old.id
            );
            changes.push((migration, false));
        }
        changes
    } else {
        migrations
            .iter()
            .filter(|m| m.id > current && m.id <= target)
            .map(|m| (m, true))
            .collect()
    };
    Ok((target, changes))
}

impl SqlStore {
    /// Apply the whole transition and its history atomically. Only hosts call this.
    pub fn migrate(&self, migrations: &[Migration], target: Option<u64>) -> Result<()> {
        self.operation("migrate", || {
            validate_migrations(migrations, target)?;
            self.migrate_before(migrations, target, Instant::now() + MIGRATION_TIMEOUT)
        })
    }

    fn migrate_before(
        &self,
        migrations: &[Migration],
        target: Option<u64>,
        deadline: Instant,
    ) -> Result<()> {
        let mut connection = self
            .connection
            .acquire(deadline, true)
            .context("waiting to migrate SQL database")?;
        let transaction = Transaction::begin(&mut connection, true)?;
        let history = history(transaction.connection)?;
        let (target, changes) = plan(migrations, &history, target)?;
        for (migration, apply) in changes {
            ensure!(Instant::now() < deadline, "migration execution timed out");
            {
                let restricted = Restricted::new(transaction.connection, true, deadline)?;
                let sql = if apply {
                    &migration.apply
                } else {
                    migration.rollback.as_ref().expect("validated rollback")
                };
                restricted.0.execute_batch(sql).with_context(|| {
                    format!(
                        "{} migration {}-{}",
                        if apply { "applying" } else { "rolling back" },
                        migration.id,
                        migration.name
                    )
                })?;
            }
            if apply {
                transaction.connection.execute("INSERT INTO _wr_migrations (id, name, apply_hash, rollback_hash, applied) VALUES (?1, ?2, ?3, ?4, 1) ON CONFLICT(id) DO UPDATE SET applied = 1", params![migration.id as i64, migration.name, hash(&migration.apply), migration.rollback.as_deref().map(hash)])?;
            } else {
                transaction.connection.execute(
                    "UPDATE _wr_migrations SET applied = 0 WHERE id = ?1",
                    [migration.id as i64],
                )?;
            }
        }
        ensure!(Instant::now() < deadline, "migration execution timed out");
        transaction.commit()?;
        self.migration.store(target, Ordering::Release);
        Ok(())
    }
}
