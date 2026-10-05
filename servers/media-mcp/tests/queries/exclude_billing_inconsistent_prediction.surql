DELETE media_usage
WHERE task = $task; UPDATE ONLY $job SET provider_payload.id = 'inconsistent', provider_payload.model = NONE
RETURN NONE;
