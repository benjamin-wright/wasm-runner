use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    time::Instant,
};

use rusqlite::{Connection, OpenFlags};

use super::{SqlError, connection};

pub const MAX_READ_CONNECTIONS: usize = 16;

struct State {
    idle: Vec<Connection>,
    opened: usize,
}

pub(super) struct ConnectionPool {
    state: Mutex<State>,
    available: Condvar,
    read_path: Option<PathBuf>,
    capacity: usize,
}

impl ConnectionPool {
    pub(super) fn writer(connection: Connection) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                idle: vec![connection],
                opened: 1,
            }),
            available: Condvar::new(),
            read_path: None,
            capacity: 1,
        })
    }

    pub(super) fn readers(path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                idle: Vec::new(),
                opened: 0,
            }),
            available: Condvar::new(),
            read_path: Some(path),
            capacity: MAX_READ_CONNECTIONS,
        })
    }

    pub(super) fn acquire(
        self: &Arc<Self>,
        deadline: Instant,
        wait: bool,
    ) -> Result<ConnectionLease, SqlError> {
        let mut state = self.state.lock().map_err(|_| SqlError::Storage)?;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(SqlError::Timeout);
            }
            if let Some(connection) = state.idle.pop() {
                return Ok(ConnectionLease {
                    owner: self.clone(),
                    connection: Some(connection),
                });
            }
            if self.read_path.is_none() && state.opened == 0 {
                return Err(SqlError::Storage);
            }
            if state.opened < self.capacity {
                state.opened += 1;
                drop(state);
                let opened = self.open_reader();
                if opened.is_err() {
                    self.state.lock().expect("SQL pool").opened -= 1;
                    self.available.notify_all();
                }
                return opened.map(|connection| ConnectionLease {
                    owner: self.clone(),
                    connection: Some(connection),
                });
            }
            if !wait {
                return Err(SqlError::Limit);
            }
            (state, _) = self
                .available
                .wait_timeout(state, remaining)
                .map_err(|_| SqlError::Storage)?;
        }
    }

    fn open_reader(&self) -> Result<Connection, SqlError> {
        let connection = Connection::open_with_flags(
            self.read_path.as_ref().expect("read connection factory"),
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection::configure(&connection)?;
        Ok(connection)
    }
}

pub(super) struct ConnectionLease {
    owner: Arc<ConnectionPool>,
    connection: Option<Connection>,
}

impl Deref for ConnectionLease {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.connection.as_ref().expect("leased SQL connection")
    }
}

impl DerefMut for ConnectionLease {
    fn deref_mut(&mut self) -> &mut Connection {
        self.connection.as_mut().expect("leased SQL connection")
    }
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        let mut connection = self.connection.take().expect("leased SQL connection");
        let clean =
            connection::clear_restrictions(&mut connection).is_ok() && connection.is_autocommit();
        let mut state = self.owner.state.lock().expect("SQL pool");
        if clean {
            state.idle.push(connection);
        } else {
            state.opened -= 1;
        }
        drop(state);
        self.owner.available.notify_all();
    }
}
