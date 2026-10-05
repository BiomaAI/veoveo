DELETE media_usage
WHERE task = $task; UPDATE ONLY $job SET provider = 'other', provider_payload.model = NONE
RETURN NONE;
