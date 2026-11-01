use std::{
    ffi::{CStr, CString},
    ptr::{self, NonNull},
    sync::Arc,
    time::Instant,
};

use rusqlite::{Connection, ffi};

use super::{
    MAX_ROW_BYTES, SqlError, SqlStatement, SqlStore, SqlValue, access::ConnectionLease, connection,
};

/// An invocation-scoped read snapshot. Closing or dropping it releases its reader.
pub struct SqlCursor {
    owner: Arc<SqlStore>,
    active: Option<ActiveCursor>,
    columns: Vec<String>,
    buffered: Option<Vec<SqlValue>>,
    deadline: Instant,
}

impl SqlCursor {
    pub(super) fn open(
        owner: Arc<SqlStore>,
        mut lease: ConnectionLease,
        input: &SqlStatement,
        deadline: Instant,
    ) -> Result<Self, SqlError> {
        connection::validate_input(input)?;
        connection::restrict(&mut lease, false, deadline)?;
        let statement = NativeStatement::prepare(&lease, input)?;
        let mut active = ActiveCursor {
            statement,
            _connection: lease,
        };
        if Instant::now() >= deadline {
            return Err(SqlError::Timeout);
        }
        // Establish the snapshot and allow SQLite to reprepare after schema changes
        // before reading column metadata. Retain at most this first row.
        let buffered = active.statement.step()?;
        if Instant::now() >= deadline {
            return Err(SqlError::Timeout);
        }
        let columns = active.statement.columns()?;
        let active = buffered.as_ref().map(|_| active);
        Ok(Self {
            owner,
            active,
            columns,
            buffered,
            deadline,
        })
    }

    pub fn columns(&self) -> &[String] {
        &self.columns
    }

    pub fn next_row(&mut self) -> Result<Option<Vec<SqlValue>>, SqlError> {
        let owner = self.owner.clone();
        owner.operation("next", || {
            if self.active.is_none() && self.buffered.is_none() {
                return Ok(None);
            }
            if Instant::now() >= self.deadline {
                self.close();
                return Err(SqlError::Timeout);
            }
            if let Some(row) = self.buffered.take() {
                return Ok(Some(row));
            }
            self.step()
        })
    }

    pub fn close(&mut self) {
        self.buffered = None;
        self.active = None;
    }

    fn step(&mut self) -> Result<Option<Vec<SqlValue>>, SqlError> {
        if Instant::now() >= self.deadline {
            self.close();
            return Err(SqlError::Timeout);
        }
        let result = self
            .active
            .as_mut()
            .expect("active cursor")
            .statement
            .step();
        if Instant::now() >= self.deadline {
            self.close();
            return Err(SqlError::Timeout);
        }
        if !matches!(&result, Ok(Some(_))) {
            self.close();
        }
        result
    }
}

impl Drop for SqlCursor {
    fn drop(&mut self) {
        self.close();
    }
}

struct ActiveCursor {
    // Declaration order finalizes the statement before returning its connection.
    statement: NativeStatement,
    _connection: ConnectionLease,
}

// SAFETY: the statement and its exclusively leased connection move together. No
// pointer or connection reference escapes this module; stepping requires &mut.
// SQLite permits connections/derived objects to move between threads, provided
// they are never used concurrently. ActiveCursor deliberately remains !Sync.
unsafe impl Send for ActiveCursor {}

// The C statement API is confined here because rusqlite's borrowed Rows cannot
// outlive an individual host call. The surrounding lease owns its connection.
struct NativeStatement(NonNull<ffi::sqlite3_stmt>);

impl NativeStatement {
    fn prepare(connection: &Connection, input: &SqlStatement) -> Result<Self, SqlError> {
        let sql = CString::new(input.sql.as_str()).map_err(|_| SqlError::InvalidStatement)?;
        let mut offset = 0;
        let mut prepared = None;
        while offset < sql.as_bytes().len() {
            let mut raw = ptr::null_mut();
            let mut tail = ptr::null();
            // SAFETY: SQL is NUL-terminated, length-bounded, and the connection is
            // exclusively leased. SQLite writes valid output pointers or NULL.
            let code = unsafe {
                ffi::sqlite3_prepare_v3(
                    connection.handle(),
                    sql.as_ptr().add(offset),
                    -1,
                    0,
                    &mut raw,
                    &mut tail,
                )
            };
            let statement = NonNull::new(raw).map(Self);
            check(code)?;
            if let Some(statement) = statement {
                if prepared.is_some() {
                    return Err(SqlError::InvalidStatement);
                }
                prepared = Some(statement);
            }
            let next = tail as usize - sql.as_ptr() as usize;
            if next <= offset {
                return Err(SqlError::InvalidStatement);
            }
            offset = next;
        }
        let statement = prepared.ok_or(SqlError::InvalidStatement)?;
        // SAFETY: this is a live prepared statement on the exclusively held connection.
        unsafe {
            if ffi::sqlite3_stmt_readonly(statement.0.as_ptr()) == 0
                || ffi::sqlite3_column_count(statement.0.as_ptr()) == 0
            {
                return Err(SqlError::InvalidStatement);
            }
            if ffi::sqlite3_bind_parameter_count(statement.0.as_ptr()) as usize
                != input.parameters.len()
            {
                return Err(SqlError::InvalidStatement);
            }
        }
        for (index, value) in input.parameters.iter().enumerate() {
            statement.bind(index as i32 + 1, value)?;
        }
        Ok(statement)
    }

    fn bind(&self, index: i32, value: &SqlValue) -> Result<(), SqlError> {
        // SAFETY: indices and payload lengths were validated before preparing.
        // TRANSIENT copies all borrowed payloads before this call returns.
        let code = unsafe {
            let statement = self.0.as_ptr();
            match value {
                SqlValue::Null => ffi::sqlite3_bind_null(statement, index),
                SqlValue::Integer(value) => ffi::sqlite3_bind_int64(statement, index, *value),
                SqlValue::Real(value) => ffi::sqlite3_bind_double(statement, index, *value),
                SqlValue::Text(value) => ffi::sqlite3_bind_text(
                    statement,
                    index,
                    value.as_ptr().cast(),
                    value.len() as i32,
                    ffi::SQLITE_TRANSIENT(),
                ),
                SqlValue::Bytes(value) if value.is_empty() => {
                    ffi::sqlite3_bind_zeroblob(statement, index, 0)
                }
                SqlValue::Bytes(value) => ffi::sqlite3_bind_blob(
                    statement,
                    index,
                    value.as_ptr().cast(),
                    value.len() as i32,
                    ffi::SQLITE_TRANSIENT(),
                ),
            }
        };
        check(code)
    }

    fn columns(&self) -> Result<Vec<String>, SqlError> {
        let mut columns = Vec::new();
        let mut bytes = 0;
        // SAFETY: column metadata is read from the live statement and copied
        // before the statement can be stepped, reset, or finalized.
        unsafe {
            for index in 0..ffi::sqlite3_column_count(self.0.as_ptr()) {
                let name = ffi::sqlite3_column_name(self.0.as_ptr(), index);
                if name.is_null() {
                    return Err(SqlError::Storage);
                }
                let name = CStr::from_ptr(name)
                    .to_str()
                    .map_err(|_| SqlError::InvalidStatement)?;
                bytes += name.len();
                if bytes > MAX_ROW_BYTES {
                    return Err(SqlError::Limit);
                }
                columns.push(name.to_owned());
            }
        }
        Ok(columns)
    }

    fn step(&mut self) -> Result<Option<Vec<SqlValue>>, SqlError> {
        // SAFETY: exclusive ownership prevents concurrent use of the statement
        // or connection; the owning ActiveCursor keeps both alive.
        let code = unsafe { ffi::sqlite3_step(self.0.as_ptr()) };
        match code {
            ffi::SQLITE_DONE => Ok(None),
            ffi::SQLITE_ROW => self.row().map(Some),
            code => {
                check(code)?;
                Err(SqlError::Storage)
            }
        }
    }

    fn row(&self) -> Result<Vec<SqlValue>, SqlError> {
        let mut values = Vec::new();
        let mut bytes = 0;
        // SAFETY: only called after SQLITE_ROW. Valid column indices are used;
        // variable-width values are copied before further stepping/finalization.
        unsafe {
            let statement = self.0.as_ptr();
            for index in 0..ffi::sqlite3_column_count(statement) {
                let kind = ffi::sqlite3_column_type(statement, index);
                let value = match kind {
                    ffi::SQLITE_NULL => SqlValue::Null,
                    ffi::SQLITE_INTEGER => {
                        SqlValue::Integer(ffi::sqlite3_column_int64(statement, index))
                    }
                    ffi::SQLITE_FLOAT => {
                        SqlValue::Real(ffi::sqlite3_column_double(statement, index))
                    }
                    ffi::SQLITE_TEXT | ffi::SQLITE_BLOB => {
                        let data = if kind == ffi::SQLITE_TEXT {
                            ffi::sqlite3_column_text(statement, index)
                        } else {
                            ffi::sqlite3_column_blob(statement, index).cast()
                        };
                        let length = ffi::sqlite3_column_bytes(statement, index) as usize;
                        bytes += length;
                        if bytes > MAX_ROW_BYTES {
                            return Err(SqlError::Limit);
                        }
                        if data.is_null() && (length != 0 || kind == ffi::SQLITE_TEXT) {
                            return Err(SqlError::Storage);
                        }
                        let data = if length == 0 {
                            &[]
                        } else {
                            std::slice::from_raw_parts(data, length)
                        };
                        if kind == ffi::SQLITE_TEXT {
                            SqlValue::Text(
                                std::str::from_utf8(data)
                                    .map_err(|_| SqlError::InvalidStatement)?
                                    .to_owned(),
                            )
                        } else {
                            SqlValue::Bytes(data.to_vec())
                        }
                    }
                    _ => return Err(SqlError::Storage),
                };
                if !matches!(value, SqlValue::Text(_) | SqlValue::Bytes(_)) {
                    bytes += 8;
                }
                if bytes > MAX_ROW_BYTES {
                    return Err(SqlError::Limit);
                }
                values.push(value);
            }
        }
        Ok(values)
    }
}

impl Drop for NativeStatement {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns the statement uniquely and finalizes it once,
        // before its ActiveCursor releases the associated connection.
        unsafe {
            ffi::sqlite3_finalize(self.0.as_ptr());
        }
    }
}

fn check(code: i32) -> Result<(), SqlError> {
    if code == ffi::SQLITE_OK {
        return Ok(());
    }
    Err(rusqlite::Error::SqliteFailure(ffi::Error::new(code), None).into())
}
