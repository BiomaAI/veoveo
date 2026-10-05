DELETE media_usage WHERE task = $task;
UPDATE ONLY $job SET terminal_event = provider_event:missing RETURN NONE;
