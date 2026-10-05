DELETE media_usage WHERE task = $task;
-- The paired fields must both be absent when no terminal observation is retained.
UPDATE ONLY $job SET terminal_event = NONE, observed_at = NONE RETURN NONE;
