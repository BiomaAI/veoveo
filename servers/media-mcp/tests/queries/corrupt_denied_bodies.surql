UPDATE ONLY $task SET request.input = { native: task_json_probe:opaque }
RETURN NONE; UPDATE ONLY $job SET provider_payload.model = NONE
RETURN NONE;
