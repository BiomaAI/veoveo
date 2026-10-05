SELECT id, partition, draft, target_ref, profile_lookup, target_lookup, detail_lookup, recorded_at FROM $ids WHERE partition = $partition;
