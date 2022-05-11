use std::time::SystemTime;
use rocksdb::{DB, DBCompactionStyle, DBCompressionType, Options};
use uuid::Uuid;

fn main() {
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

    let db = DB::open(&opts, "./rocks.db")
        .expect("failed to open rocks db");

    let total_keys = 2000000_u64;

    let mut keys: Vec<[u8; 32]> = Vec::new();

    let key_ns = Uuid::parse_str("21a117c5-8ec5-417f-974a-9ff9441f754d").unwrap();

    for i in 0..total_keys {
        let i_bytes = i.to_be_bytes();
        let cur_key = Uuid::new_v5(&key_ns, &i_bytes);

        let mut val: [u8; 32] = Default::default();
        val.copy_from_slice(format!("{}", cur_key.to_simple().to_string()).as_bytes());

        keys.push(val);
    }

    let start = SystemTime::now();

    for k in keys.iter() {
        db.put(k, k)
            .expect("failed to put");
    }

    db.flush().expect("failed to flush");

    let end = SystemTime::now();
    let elapsed = end.duration_since(start);
    let taken_ms = elapsed.unwrap_or_default().as_millis();

    println!("rocks set: {}ms ({}/sec)", taken_ms, (total_keys  * 1000) as u128 / taken_ms);

    let start = SystemTime::now();

    for k in keys.iter() {
        let _val = db.get(k).expect("failed to get");
    }

    let end = SystemTime::now();
    let elapsed = end.duration_since(start);
    let taken_ms = elapsed.unwrap_or_default().as_millis();

    println!("rocks get: {}ms ({}/sec)", taken_ms, (total_keys  * 1000) as u128 / taken_ms);
}

#[cfg(test)]
mod tests {
    //use std::borrow::Borrow;
    //use std::{fs};
    //use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
    use std::time::SystemTime;
    use uuid::Uuid;

    //use indradb::{BulkInsertItem, Datastore, EdgeKey, Transaction, Vertex, VertexQueryExt};
    use rocksdb::{DB, DBCompactionStyle, Options};

    /*
    #[test]
    fn sled_test() {
        let db = sled::open("./sled.db")
            .expect("failed to open sled db");

        let total_keys = 2000000_u64;

        let mut keys: Vec<[u8; 32]> = Vec::new();

        let key_ns = Uuid::parse_str("21a117c5-8ec5-417f-974a-9ff9441f754d").unwrap();

        for i in 0..total_keys {
            let cur_key = Uuid::new_v5(&key_ns, i.to_be_bytes().as_slice());

            let mut val: [u8; 32] = Default::default();
            val.copy_from_slice(format!("{}", cur_key.to_simple().to_string()).as_bytes());

            keys.push(val);
        }

        let start = SystemTime::now();

        let mut batch = sled::Batch::default();

        for k in keys.iter() {
            batch.insert(k, k)
                //.expect("failed to insert");
        }

        db.apply_batch(batch)
            .expect("failed to apply batch");

        db.flush().expect("failed to flush");

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        println!("sled set: {taken_ms}ms ({}/sec)", (total_keys  * 1000) as u128 / taken_ms);

        let start = SystemTime::now();

        for k in keys.iter() {
            let _val = db.get(k).expect("failed to get");
        }

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        println!("sled get: {taken_ms}ms ({}/sec)", (total_keys  * 1000) as u128 / taken_ms);
    }

     */

    #[test]
    fn rocks_test() {
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

        let db = DB::open(&opts, "./rocks.db")
            .expect("failed to open rocks db");

        let total_keys = 2000000_u64;

        let mut keys: Vec<[u8; 32]> = Vec::new();

        let key_ns = Uuid::parse_str("21a117c5-8ec5-417f-974a-9ff9441f754d").unwrap();

        for i in 0..total_keys {
            let cur_key = Uuid::new_v5(&key_ns, i.to_be_bytes().as_slice());

            let mut val: [u8; 32] = Default::default();
            val.copy_from_slice(format!("{}", cur_key.to_simple().to_string()).as_bytes());

            keys.push(val);
        }

        /*
        let start = SystemTime::now();

        for k in keys.iter() {
            db.put(k, k)
                .expect("failed to put");
        }

        db.flush().expect("failed to flush");

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        println!("rocks set: {taken_ms}ms ({}/sec)", (total_keys  * 1000) as u128 / taken_ms);

         */
        let start = SystemTime::now();

        for k in keys.iter() {
            let _val = db.get(k).expect("failed to get");
        }

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        println!("rocks get: {taken_ms}ms ({}/sec)", (total_keys  * 1000) as u128 / taken_ms);
    }

    /*
    #[test]
    fn leveldb_test() {
        let opt = rusty_leveldb::Options::default();
        let mut db = DB::open("./level.db", opt)
            .expect("failed to open sled db");

        let total_keys = 2000000_u64;

        let mut keys: Vec<[u8; 32]> = Vec::new();

        let key_ns = Uuid::parse_str("21a117c5-8ec5-417f-974a-9ff9441f754d").unwrap();

        for i in 0..total_keys {
            let cur_key = Uuid::new_v5(&key_ns, i.to_be_bytes().as_slice());

            let mut val: [u8; 32] = Default::default();
            val.copy_from_slice(format!("{}", cur_key.to_simple().to_string()).as_bytes());

            keys.push(val);
        }

        let start = SystemTime::now();

        for k in keys.iter() {
            db.put(k, k).unwrap();
        }

        db.flush().unwrap();

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        println!("leveldb set: {taken_ms}ms ({}/sec)", (total_keys  * 1000) as u128 / taken_ms);

        let start = SystemTime::now();

        db.compact_range(keys.iter().next().unwrap(),
                         keys.iter().next_back().unwrap()).unwrap();

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        println!("leveldb compacted: {taken_ms}ms ({}/sec)", (total_keys  * 1000) as u128 / taken_ms);

        let start = SystemTime::now();

        for k in keys.iter() {
            let _val = db.get(k).unwrap().as_slice();
        }

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        println!("leveldb get: {taken_ms}ms ({}/sec)", (total_keys  * 1000) as u128 / taken_ms);
    }
     */

    /*
    #[test]
    fn create_and_read_graph() {
        let total_users = 5000_u64;
        //let total_users = 4_u64;
        let total_friends = 500_u64;
        //let total_friends = 2_u64;

        // Indra
        //let person_type = indradb::Identifier::new("person").unwrap(); // Entity
        let person_type = indradb::Type::new("person").unwrap(); // Entity
        //let friend_type = indradb::Identifier::new("friend").unwrap(); // Link
        let friend_type = indradb::Type::new("friend").unwrap(); // Link

        let start = SystemTime::now();

        //let mut ds = indradb::RocksdbDatastore::new("./indra.rocks.db", None)
        //    .expect("failed to create indra rocks db");

        let ds = indradb_sled::SledDatastore::new("./indra.sled.db")
            .expect("failed to create indra sled db");

        //let mut ds = indradb::MemoryDatastore::create("/tmp/indra.db")
        //    .expect("failed to create indra ds");

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        println!("graph loaded: {taken_ms}ms");

        let user_ns = Uuid::parse_str("15365a25cd2f4daca54e9087ab23aef1")
            .expect("failed to parse UUID");
        let friend_ns = Uuid::parse_str("5a032842eecb4477a651feda1a0e00a3")
            .expect("failed to parse UUID"); // Ensure different for testing!

        // Pre-create the UUIDs.
        let mut users: Vec<Uuid> = Vec::new();
        for seq in 0..total_users {
            users.push(Uuid::new_v5(&user_ns,
                                    seq.to_be_bytes().as_slice()))
        }

        let mut friends: Vec<Uuid> = Vec::new();
        for seq in 0..total_friends {
            friends.push(Uuid::new_v5(&friend_ns,
                                    seq.to_be_bytes().as_slice()))
        }

        // Generate
        /*
        let start = SystemTime::now();

        let mut inserts: Vec<BulkInsertItem> = Vec::new();

        for f in friends.iter() {
            inserts.push(BulkInsertItem::Vertex(Vertex::with_id(f.clone(), person_type.clone())));
        }

        for u in users.iter() {
            inserts.push(BulkInsertItem::Vertex(Vertex::with_id(u.clone(), person_type.clone())));

            for f in friends.iter() {
                let edge_key = EdgeKey::new(u.clone(),
                                            friend_type.clone(), f.clone());

                // Friend connections are bi-directional.
                inserts.push(BulkInsertItem::Edge(edge_key.reversed()));
                inserts.push(BulkInsertItem::Edge(edge_key));
            }
        }

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        println!("graph generated: {taken_ms}ms");

        // Insert
        let start = SystemTime::now();

        ds.bulk_insert(inserts.into_iter()).expect("failed to insert");

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        println!("graph inserted: {taken_ms}ms");

         */

        let start = SystemTime::now();

        let tx = ds.transaction()
            .expect("failed to get transaction");

        for u in users.iter() {
            let specific_u = indradb::SpecificVertexQuery::single(u.clone());

            let _edges = tx.get_edges(specific_u.outbound().t(friend_type.clone()))
                .expect("failed to get edges");
        }

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        println!("graph fetched: {taken_ms}ms");

        //sleep(time::Duration::from_millis(10000));
    }
     */

    /*
    #[test]
    fn create_and_read_feed() {
        let header = "DUMMY[head:0]"; // Header to store current head position.
        let header_len = header.len() as u64;
        let data = "AA91a108816e1d439083a1d9ba36a5a398"; // 2 byte type, 32 byte UUID.
        let data_len = data.len() as u64;

        let total_feeds = 200_u64;
        let total_entries = 10000_u64;
        let fetch_size = 50_u64;

        let file_name = "/home/david/teggle/feeds.dat";

        create_feed_file(&file_name, &header, data_len, total_feeds, total_entries);
        write_feed_file(&file_name, &header, &data, total_feeds, total_entries);
        read_feed_file(&file_name, header_len, data_len, total_feeds, total_entries, fetch_size);
    }

    // Testing out feed db
    // A lot of copied code here, obviously won't be like this.

    fn create_feed_file(
        file_name: &str,
        header: &str,
        data_len: u64,
        total_feeds: u64,
        total_entries: u64
    ) {
        let header_len = header.len() as u64;
        let page_size = header_len + (total_entries * data_len);

        let file_path = std::path::Path::new(file_name);
        if file_path.exists() {
            fs::remove_file(file_path)
                .expect("failed to remove file");
        }

        let mut file = BufWriter::new(fs::OpenOptions::new()
            .write(true)
            .create(true)
            .open(&file_path)
            .unwrap());

        let start = SystemTime::now();

        for fi in 0..total_feeds {
            file.seek(SeekFrom::Start(fi * page_size))
                .expect("failed to seek");
            file.write_all(header.as_bytes())
                .expect("failed to write");
        }

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        let total_entries = total_feeds * total_entries;
        if taken_ms <= 0 {
            println!("created feed file: < {taken_ms}ms ({}+/ms)", total_entries)
        } else {
            println!("created feed file: {taken_ms}ms ({}/ms)", total_entries as u128 / taken_ms)
        }
    }

    fn write_feed_file(
        file_name: &str,
        header: &str,
        data: &str,
        total_feeds: u64,
        total_entries: u64
    ) {
        let header_len = header.len() as u64;
        let data_len = data.len() as u64;

        let page_size = header_len + (total_entries * data_len);

        let mut file = BufWriter::new(fs::OpenOptions::new()
            .write(true)
            .read(true)
            .open(file_name)
            .unwrap());

        let start = SystemTime::now();

        for ti in 0..total_entries {
            for fi in 0..total_feeds {
                let page_pos = header_len + (fi * page_size);

                file.seek(SeekFrom::Start(page_pos + (ti * data_len)))
                    .expect("failed to seek");
                file.write_all(data.as_bytes())
                    .expect("failed to write");
            }
        }

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        let total_entries = total_feeds * total_entries;
        if taken_ms <= 0 {
            println!("wrote feed file: < {taken_ms}ms ({}+/ms)", total_entries)
        } else {
            println!("wrote feed file: {taken_ms}ms ({}/s)", (total_entries as u128 * 1000) / taken_ms)
        }
    }

    fn read_feed_file(
        file_name: &str,
        header_len: u64,
        data_len: u64,
        total_feeds: u64,
        total_entries: u64,
        fetch_size: u64
    ) {
        let page_size = header_len + (total_entries * data_len);
        let fetch_size_bytes = fetch_size * data_len;

        let mut file = BufReader::new(fs::OpenOptions::new()
            .read(true)
            .open(file_name)
            .unwrap());

        let start = SystemTime::now();

        for fei in 0..(total_entries / fetch_size) {
            for fi in 0..total_feeds {
                let page_pos = header_len + (fi * page_size);
                let fetch_pos = fei * fetch_size_bytes;
                let seek_pos = page_pos + fetch_pos;

                file.seek(SeekFrom::Start(seek_pos))
                    .expect("failed to seek");

                let mut buf = vec![0u8; fetch_size_bytes as usize];
                file.read_exact(&mut buf)
                    .expect("failed to read");
            }
        }

        let end = SystemTime::now();
        let elapsed = end.duration_since(start);
        let taken_ms = elapsed.unwrap_or_default().as_millis();

        let total_entries = total_feeds * total_entries;
        if taken_ms <= 0 {
            println!("read feed file: < {taken_ms}ms ({}+/ms)", total_entries)
        } else {
            println!("read feed file: {taken_ms}ms ({}/s)", (total_entries as u128 * 1000) / taken_ms)
        }
    }

     */
}
