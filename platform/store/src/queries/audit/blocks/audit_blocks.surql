SELECT id, partition, sequence, head_hash, block FROM audit_block:[$partition, 0]..=[$partition, 9223372036854775807] WHERE partition = $partition AND sequence > $after ORDER BY id ASC LIMIT $limit;
