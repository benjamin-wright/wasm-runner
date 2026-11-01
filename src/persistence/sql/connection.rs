use std::time::Instant;

use rusqlite::{
    Batch, Connection, Statement,
    config::DbConfig,
    fallible_iterator::FallibleIterator,
    hooks::{AuthAction, AuthContext, Authorization},
    limits::Limit,
    types::Value,
};

use super::{MAX_ROW_BYTES, MAX_SQL_BYTES, SqlError, SqlStatement, SqlValue};

pub(super) fn configure(connection: &Connection) -> rusqlite::Result<()> {
    connection.busy_timeout(std::time::Duration::ZERO)?;
    connection.execute_batch(
        "PRAGMA synchronous = FULL; PRAGMA foreign_keys = ON; PRAGMA temp_store = MEMORY;",
    )?;
    connection.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    connection.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)?;
    connection.set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DDL, false)?;
    connection.set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DML, false)?;
    for (limit, value) in [
        (Limit::SQLITE_LIMIT_LENGTH, MAX_ROW_BYTES as i32),
        (Limit::SQLITE_LIMIT_SQL_LENGTH, MAX_SQL_BYTES as i32),
        (Limit::SQLITE_LIMIT_COLUMN, 128),
        (Limit::SQLITE_LIMIT_VARIABLE_NUMBER, 256),
        (Limit::SQLITE_LIMIT_ATTACHED, 0),
        (Limit::SQLITE_LIMIT_EXPR_DEPTH, 100),
        (Limit::SQLITE_LIMIT_COMPOUND_SELECT, 50),
        (Limit::SQLITE_LIMIT_TRIGGER_DEPTH, 16),
        (Limit::SQLITE_LIMIT_VDBE_OP, 100_000),
    ] {
        connection.set_limit(limit, value)?;
    }
    Ok(())
}

pub(super) struct Restricted<'a>(pub(super) &'a mut Connection);

impl<'a> Restricted<'a> {
    pub(super) fn new(
        connection: &'a mut Connection,
        migration: bool,
        deadline: Instant,
    ) -> rusqlite::Result<Self> {
        let guard = Self(connection);
        restrict(guard.0, migration, deadline)?;
        Ok(guard)
    }
}

impl Drop for Restricted<'_> {
    fn drop(&mut self) {
        clear_restrictions(self.0).expect("clearing SQL restrictions");
    }
}

pub(super) fn restrict(
    connection: &mut Connection,
    migration: bool,
    deadline: Instant,
) -> rusqlite::Result<()> {
    connection.authorizer(Some(move |context: AuthContext<'_>| {
        authorize(context, migration)
    }))?;
    connection.progress_handler(1000, Some(move || Instant::now() >= deadline))?;
    Ok(())
}

pub(super) fn clear_restrictions(connection: &mut Connection) -> rusqlite::Result<()> {
    connection.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)?;
    connection.progress_handler(0, None::<fn() -> bool>)?;
    Ok(())
}

fn protected(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with("_wr_")
}

fn authorize(context: AuthContext<'_>, migration: bool) -> Authorization {
    use AuthAction::*;
    let allowed = match context.action {
        Read { table_name, .. }
        | Insert { table_name }
        | Update { table_name, .. }
        | Delete { table_name } => !protected(table_name),
        Select | Recursive => true,
        Function { function_name } => {
            let name = function_name.to_ascii_lowercase();
            !matches!(
                name.as_str(),
                "load_extension" | "readfile" | "writefile" | "edit" | "fts3_tokenizer"
            ) && !name.starts_with("pragma_")
        }
        CreateTable { table_name }
        | DropTable { table_name }
        | AlterTable { table_name, .. }
        | Analyze { table_name } => migration && !protected(table_name),
        CreateIndex {
            index_name,
            table_name,
        }
        | DropIndex {
            index_name,
            table_name,
        } => migration && !protected(table_name) && !protected(index_name),
        CreateTrigger {
            trigger_name,
            table_name,
        }
        | DropTrigger {
            trigger_name,
            table_name,
        } => migration && !protected(table_name) && !protected(trigger_name),
        CreateView { view_name } | DropView { view_name } => migration && !protected(view_name),
        Reindex { index_name } => migration && !protected(index_name),
        _ => false,
    };
    if allowed {
        Authorization::Allow
    } else {
        Authorization::Deny
    }
}

pub(super) fn validate_input(input: &SqlStatement) -> Result<(), SqlError> {
    let parameter_bytes: usize = input
        .parameters
        .iter()
        .map(|value| match value {
            SqlValue::Text(value) => value.len(),
            SqlValue::Bytes(value) => value.len(),
            _ => 8,
        })
        .sum();
    if input.sql.len() > MAX_SQL_BYTES
        || parameter_bytes > MAX_ROW_BYTES
        || input.parameters.len() > 256
    {
        return Err(SqlError::Limit);
    }
    if input.sql.contains('\0')
        || input
            .parameters
            .iter()
            .any(|value| matches!(value, SqlValue::Real(value) if !value.is_finite()))
    {
        return Err(SqlError::InvalidStatement);
    }
    Ok(())
}

fn prepare<'a>(
    connection: &'a Connection,
    input: &SqlStatement,
) -> Result<Statement<'a>, SqlError> {
    validate_input(input)?;
    let mut batch = Batch::new(connection, &input.sql);
    let mut statement = batch.next()?.ok_or(SqlError::InvalidStatement)?;
    if batch.next()?.is_some() || statement.parameter_count() != input.parameters.len() {
        return Err(SqlError::InvalidStatement);
    }
    for (index, value) in input.parameters.iter().enumerate() {
        let value = match value {
            SqlValue::Null => Value::Null,
            SqlValue::Integer(value) => Value::Integer(*value),
            SqlValue::Real(value) if value.is_finite() => Value::Real(*value),
            SqlValue::Real(_) => return Err(SqlError::InvalidStatement),
            SqlValue::Text(value) => Value::Text(value.clone()),
            SqlValue::Bytes(value) => Value::Blob(value.clone()),
        };
        statement.raw_bind_parameter(index + 1, value)?;
    }
    Ok(statement)
}

pub(super) fn execute(connection: &Connection, input: &SqlStatement) -> Result<u64, SqlError> {
    let mut statement = prepare(connection, input)?;
    if statement.column_count() != 0 {
        return Err(SqlError::InvalidStatement);
    }
    Ok(statement.raw_execute()? as u64)
}

// Own rollback even when authorization or deadline checks return early.
pub(super) struct Transaction<'a> {
    pub(super) connection: &'a mut Connection,
    committed: bool,
}

impl<'a> Transaction<'a> {
    pub(super) fn begin(connection: &'a mut Connection, immediate: bool) -> rusqlite::Result<Self> {
        connection.execute_batch(if immediate {
            "BEGIN IMMEDIATE"
        } else {
            "BEGIN"
        })?;
        Ok(Self {
            connection,
            committed: false,
        })
    }

    pub(super) fn commit(mut self) -> rusqlite::Result<()> {
        self.connection.execute_batch("COMMIT")?;
        self.committed = true;
        Ok(())
    }
}

impl Drop for Transaction<'_> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.connection.execute_batch("ROLLBACK");
        }
    }
}
