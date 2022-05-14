use lazy_static::lazy_static;
use super::traits::{Db, Result};

pub mod rocksdb;

lazy_static! {
    // TOOD: Make better.
    pub static ref GLOBAL_DB: DbInstance<rocksdb::RocksDb> = DbInstance::new(
        rocksdb::RocksDb::default().expect("failed to init rocks db")
    );
}

pub struct DbInstance<D: Db> {
    db: D
}

impl <D: Db> DbInstance<D> {
    pub fn new(db: D) -> Self {
        Self { db }
    }
}

impl <D: Db> Db for DbInstance<D> {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.db.get(key)
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.db.put(key, value)
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.db.delete(key)
    }

    fn flush(&self) -> Result<()> {
        self.db.flush()
    }
}