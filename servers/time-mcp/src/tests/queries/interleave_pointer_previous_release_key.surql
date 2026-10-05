DEFINE EVENT OVERWRITE time_test_interleave ON time_authority_release
WHEN $event = 'UPDATE' AND $before.state = 'staged' AND $after.state = 'active'
THEN (UPDATE time_active_authority SET previous_release_key = NONE WHERE tenant = $after.tenant
    AND dataset_kind = $after.dataset_kind AND id != $after.id );
