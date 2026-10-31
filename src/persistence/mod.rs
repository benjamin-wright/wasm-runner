//! Persistent application-scoped key/value storage. Owners must explicitly sync
//! after draining their users before releasing the database.
mod storage;

pub use storage::{
    KvError, KvStore, KvValue, OperationStats, Storage, StorageInfo, StoreInfo, SyncInfo,
};
