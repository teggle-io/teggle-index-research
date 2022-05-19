use rocksdb::{DB, DBCompactionStyle, Options};

use crate::traits::{Db, Error, Result};

pub struct RocksDb {
    db: DB,
}

impl RocksDb {
    pub fn new(db: DB) -> Self {
        Self { db }
    }

    pub fn default() -> Result<Self> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.set_compaction_style(DBCompactionStyle::Level);
        opts.set_write_buffer_size(67_108_864); // 64mb
        opts.set_max_write_buffer_number(3);
        opts.set_target_file_size_base(67_108_864); // 64mb
        opts.set_level_zero_file_num_compaction_trigger(8);
        opts.set_level_zero_slowdown_writes_trigger(17);
        opts.set_level_zero_stop_writes_trigger(24);
        opts.set_num_levels(4);
        opts.set_max_bytes_for_level_base(536_870_912); // 512mb
        opts.set_max_bytes_for_level_multiplier(8.0);

        return match DB::open(&opts, "./rocks.db") {
            Ok(db) => {
                Ok(Self::new(db))
            }
            Err(err) => {
                Err(map_rocks_err(err))
            }
        };
    }
}

impl Db for RocksDb {
    fn get(&self, key: &[u8]) -> Result<Option<Vec<u8>>> {
        self.db.get(key).map_err(map_rocks_err)
    }

    fn put(&self, key: &[u8], value: &[u8]) -> Result<()> {
        self.db.put(key, value).map_err(map_rocks_err)
    }

    fn delete(&self, key: &[u8]) -> Result<()> {
        self.db.delete(key).map_err(map_rocks_err)
    }

    fn flush(&self) -> Result<()> {
        self.db.flush().map_err(map_rocks_err)
    }
}

// Util

fn map_rocks_err(err: rocksdb::Error) -> Error {
    Error::new(err.to_string())
}