DELETE media_usage
WHERE task = $task; UPDATE ONLY $job SET tenant = tenant:missing, provider_payload.model = NONE
RETURN NONE;
