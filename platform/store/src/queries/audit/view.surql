SELECT id, partition, draft, target_ref, profile_lookup, target_lookup, detail_lookup, recorded_at FROM ONLY $id
WHERE partition = $actor_partition
    AND actor_key = $actor
    AND profile_lookup = $profile
    AND target_lookup = $target
    AND detail_lookup = $detail
    AND outcome = 'allowed'
    AND occurred_at > time::now() - 15m;
