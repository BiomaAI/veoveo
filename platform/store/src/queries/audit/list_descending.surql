SELECT id, partition, draft, target_ref, profile_lookup, target_lookup, detail_lookup, recorded_at FROM audit_record:[$partition, u"00000000-0000-0000-0000-000000000000"]..=[$partition, u"ffffffff-ffff-ffff-ffff-ffffffffffff"]
WHERE partition = $partition
    AND ($after = NONE OR id < $after)
    AND ($class = NONE OR class = $class)
    AND ($actor = NONE OR actor_key = $actor)
    AND ($target = NONE OR target_lookup = $target)
    AND ($outcome = NONE OR outcome = $outcome)
    AND ($trace = NONE OR trace_id = $trace)
    AND ($from = NONE OR occurred_at >= $from)
    AND ($until = NONE OR occurred_at < $until)
ORDER BY id DESC LIMIT $limit;
