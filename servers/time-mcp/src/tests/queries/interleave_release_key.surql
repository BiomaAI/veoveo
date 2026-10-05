DEFINE EVENT OVERWRITE time_test_interleave ON time_authority_release
WHEN $event = 'UPDATE' AND $before.state = 'staged' AND $after.state = 'active'
THEN (UPDATE time_authority_release SET release_key = 'time-release-changed' WHERE tenant = $after.tenant
    AND dataset_kind = $after.dataset_kind AND id != $after.id AND state = 'active');
