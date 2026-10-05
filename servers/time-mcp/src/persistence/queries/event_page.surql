SELECT * FROM time_temporal_event
WHERE tenant = $tenant AND owner = $owner AND ($state = NONE OR state = $state) AND ($after_key = NONE OR due_tai_seconds_since_1970 > $after_seconds OR (due_tai_seconds_since_1970 = $after_seconds AND due_nanosecond > $after_nanosecond) OR (due_tai_seconds_since_1970 = $after_seconds AND due_nanosecond = $after_nanosecond AND event_key > $after_key))
ORDER BY due_tai_seconds_since_1970 ASC, due_nanosecond ASC, event_key ASC
LIMIT $limit;
