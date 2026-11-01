//! Persistent application-scoped KV and SQL storage. Drain users before releasing
//! capabilities; KV owners must also explicitly sync their database.
mod paths;
pub mod sql;
mod storage;

pub use storage::{
    KvError, KvStore, KvValue, OperationStats, Storage, StorageInfo, StoreInfo, SyncInfo,
};
