CREATE TABLE _wr_migrations (
    id INTEGER PRIMARY KEY CHECK (id > 0),
    name TEXT NOT NULL,
    apply_hash BLOB NOT NULL,
    rollback_hash BLOB,
    applied INTEGER NOT NULL CHECK (applied IN (0, 1))
);
