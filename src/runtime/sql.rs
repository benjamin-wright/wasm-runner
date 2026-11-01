use super::{HostState, local::sql::database as guest};
use crate::persistence::sql::{SqlCursor, SqlError, SqlStatement, SqlStore, SqlValue};
use anyhow::{Result, ensure};
use std::{sync::Arc, time::Instant};
use wasmtime::component::Resource;

const MAX_CURSORS: usize = 4;

impl HostState {
    fn sql_call<T>(
        &self,
        run: impl FnOnce(&Arc<SqlStore>, Instant) -> Result<T, SqlError>,
    ) -> Result<Result<T, guest::SqlError>> {
        ensure!(
            self.started.elapsed() < self.timeout,
            "invocation expired before SQL call"
        );
        let result = match &self.capabilities.sql {
            Some(store) => run(
                store,
                self.started
                    .checked_add(self.timeout)
                    .ok_or_else(|| anyhow::anyhow!("SQL deadline exceeds clock range"))?,
            )
            .map_err(error),
            None => Err(guest::SqlError::Disabled),
        };
        ensure!(
            self.started.elapsed() < self.timeout,
            "invocation expired after SQL call"
        );
        Ok(result)
    }
}

impl guest::Host for HostState {
    fn execute(&mut self, statement: guest::Statement) -> Result<Result<u64, guest::SqlError>> {
        self.sql_call(|store, deadline| store.execute(&input(statement), deadline))
    }

    fn query(
        &mut self,
        statement: guest::Statement,
    ) -> Result<Result<Resource<SqlCursor>, guest::SqlError>> {
        let result = self.sql_call(|store, deadline| {
            if self.sql_cursors == MAX_CURSORS {
                return Err(SqlError::Limit);
            }
            store.query_with_wait(&input(statement), deadline, self.sql_cursors == 0)
        })?;
        match result {
            Ok(cursor) => {
                let cursor = self.table.push(cursor)?;
                self.sql_cursors += 1;
                Ok(Ok(cursor))
            }
            Err(error) => Ok(Err(error)),
        }
    }

    fn batch(
        &mut self,
        statements: Vec<guest::Statement>,
    ) -> Result<Result<Vec<u64>, guest::SqlError>> {
        self.sql_call(|store, deadline| {
            store.batch(
                &statements.into_iter().map(input).collect::<Vec<_>>(),
                deadline,
            )
        })
    }
}

impl guest::HostCursor for HostState {
    fn columns(&mut self, cursor: Resource<SqlCursor>) -> Result<Vec<String>> {
        ensure!(
            self.started.elapsed() < self.timeout,
            "invocation expired before SQL cursor access"
        );
        Ok(self.table.get(&cursor)?.columns().to_vec())
    }

    fn next(
        &mut self,
        cursor: Resource<SqlCursor>,
    ) -> Result<Result<Option<Vec<guest::Value>>, guest::SqlError>> {
        let result = self
            .table
            .get_mut(&cursor)?
            .next_row()
            .map(|row| row.map(|values| values.into_iter().map(output).collect()))
            .map_err(error);
        ensure!(
            self.started.elapsed() < self.timeout,
            "invocation expired after SQL cursor access"
        );
        Ok(result)
    }

    fn drop(&mut self, cursor: Resource<SqlCursor>) -> Result<()> {
        self.table.delete(cursor)?;
        self.sql_cursors -= 1;
        Ok(())
    }
}

fn error(error: SqlError) -> guest::SqlError {
    match error {
        SqlError::InvalidStatement => guest::SqlError::InvalidStatement,
        SqlError::Denied => guest::SqlError::Denied,
        SqlError::Constraint => guest::SqlError::Constraint,
        SqlError::Busy => guest::SqlError::Busy,
        SqlError::Limit => guest::SqlError::Limit,
        SqlError::Timeout => guest::SqlError::Timeout,
        SqlError::Storage => guest::SqlError::Storage,
    }
}

fn input(statement: guest::Statement) -> SqlStatement {
    SqlStatement {
        sql: statement.sql,
        parameters: statement
            .parameters
            .into_iter()
            .map(|value| match value {
                guest::Value::Null => SqlValue::Null,
                guest::Value::Integer(value) => SqlValue::Integer(value),
                guest::Value::Real(value) => SqlValue::Real(value),
                guest::Value::Text(value) => SqlValue::Text(value),
                guest::Value::Bytes(value) => SqlValue::Bytes(value),
            })
            .collect(),
    }
}

fn output(value: SqlValue) -> guest::Value {
    match value {
        SqlValue::Null => guest::Value::Null,
        SqlValue::Integer(value) => guest::Value::Integer(value),
        SqlValue::Real(value) => guest::Value::Real(value),
        SqlValue::Text(value) => guest::Value::Text(value),
        SqlValue::Bytes(value) => guest::Value::Bytes(value),
    }
}
