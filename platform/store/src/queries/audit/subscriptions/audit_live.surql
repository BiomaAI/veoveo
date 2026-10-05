LIVE SELECT id, partition, draft, target_ref, profile_lookup, target_lookup, detail_lookup, recorded_at FROM audit_record WHERE partition = $partition;
