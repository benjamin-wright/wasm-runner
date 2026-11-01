use crate::persistence::sql::{Migration, SqlConfig, validate_migrations};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, fs, io::Read, path::Path};

pub(super) fn load(directory: &Path, config: SqlConfig) -> Result<Vec<Migration>> {
    config.validate()?;
    if !config.enabled {
        return Ok(Vec::new());
    }
    let directory = directory.join("migrations");
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            validate_migrations(&[], config.migration_target)?;
            return Ok(Vec::new());
        }
        Err(error) => return Err(error).context("reading migrations directory"),
    };
    let mut migrations: BTreeMap<u64, Migration> = BTreeMap::new();
    let mut total = 0;
    for entry in entries {
        let entry = entry?;
        ensure!(
            entry.file_type()?.is_file(),
            "migration entries must be regular files"
        );
        let filename = entry
            .file_name()
            .into_string()
            .map_err(|_| anyhow::anyhow!("invalid migration filename"))?;
        let (stem, apply) = if let Some(stem) = filename.strip_suffix(".apply.sql") {
            (stem, true)
        } else {
            (
                filename
                    .strip_suffix(".rollback.sql")
                    .with_context(|| format!("invalid migration filename {filename}"))?,
                false,
            )
        };
        let (number, name) = stem
            .split_once('-')
            .context("migration filename requires a numeric ID and name")?;
        ensure!(
            number.len() >= 4 && number.bytes().all(|b| b.is_ascii_digit()),
            "migration IDs require at least four digits"
        );
        let id = number.parse::<u64>().context("invalid migration ID")?;
        let mut sql = String::new();
        fs::File::open(entry.path())?
            .take((crate::persistence::sql::MAX_SQL_BYTES + 1) as u64)
            .read_to_string(&mut sql)?;
        ensure!(
            !sql.trim().is_empty() && sql.len() <= crate::persistence::sql::MAX_SQL_BYTES,
            "migration file {filename} is empty or exceeds 1 MiB"
        );
        total += sql.len();
        ensure!(total <= 8 * 1024 * 1024, "migration bundle exceeds 8 MiB");
        let migration = migrations.entry(id).or_insert_with(|| Migration {
            id,
            name: name.to_owned(),
            apply: String::new(),
            rollback: None,
        });
        ensure!(
            migration.name == name,
            "migration ID {id} has conflicting names"
        );
        if apply {
            ensure!(migration.apply.is_empty(), "duplicate apply migration {id}");
            migration.apply = sql;
        } else {
            ensure!(
                migration.rollback.is_none(),
                "duplicate rollback migration {id}"
            );
            migration.rollback = Some(sql);
        }
        ensure!(migrations.len() <= 1000, "too many migrations");
    }
    let migrations: Vec<_> = migrations.into_values().collect();
    validate_migrations(&migrations, config.migration_target)?;
    Ok(migrations)
}
