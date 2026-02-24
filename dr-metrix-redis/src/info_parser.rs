/// Parsed data from Redis `INFO ALL`.
#[derive(Debug, Default)]
pub struct RedisInfo {
    pub connected_clients: i64,
    pub blocked_clients: i64,
    pub used_memory: i64,
    pub used_memory_rss: i64,
    pub mem_fragmentation_ratio: f64,
    pub total_commands_processed: i64,
    pub keyspace_hits: i64,
    pub keyspace_misses: i64,
    pub evicted_keys: i64,
    pub replication_offset: i64,
    pub connected_slaves: i64,
    /// (db_index, keys, expires)
    pub keyspace: Vec<(u32, i64, i64)>,
}

/// Parse the text returned by `INFO ALL` into a [`RedisInfo`].
pub fn parse_info(raw: &str) -> RedisInfo {
    let mut info = RedisInfo::default();

    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        // Keyspace lines look like: db0:keys=15420,expires=5,avg_ttl=0
        if let Some(rest) = line.strip_prefix("db") {
            if let Some(colon) = rest.find(':') {
                let db_idx_str = &rest[..colon];
                let fields = &rest[colon + 1..];
                if let Ok(db_idx) = db_idx_str.parse::<u32>() {
                    let mut keys: i64 = 0;
                    let mut expires: i64 = 0;
                    for field in fields.split(',') {
                        if let Some(v) = field.strip_prefix("keys=") {
                            keys = v.parse().unwrap_or(0);
                        } else if let Some(v) = field.strip_prefix("expires=") {
                            expires = v.parse().unwrap_or(0);
                        }
                    }
                    info.keyspace.push((db_idx, keys, expires));
                    continue;
                }
            }
        }

        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();

        match key {
            "connected_clients" => info.connected_clients = value.parse().unwrap_or(0),
            "blocked_clients" => info.blocked_clients = value.parse().unwrap_or(0),
            "used_memory" => info.used_memory = value.parse().unwrap_or(0),
            "used_memory_rss" => info.used_memory_rss = value.parse().unwrap_or(0),
            "mem_fragmentation_ratio" => {
                info.mem_fragmentation_ratio = value.parse().unwrap_or(0.0)
            }
            "total_commands_processed" => {
                info.total_commands_processed = value.parse().unwrap_or(0)
            }
            "keyspace_hits" => info.keyspace_hits = value.parse().unwrap_or(0),
            "keyspace_misses" => info.keyspace_misses = value.parse().unwrap_or(0),
            "evicted_keys" => info.evicted_keys = value.parse().unwrap_or(0),
            "master_repl_offset" => info.replication_offset = value.parse().unwrap_or(0),
            "connected_slaves" => info.connected_slaves = value.parse().unwrap_or(0),
            _ => {}
        }
    }

    info
}
