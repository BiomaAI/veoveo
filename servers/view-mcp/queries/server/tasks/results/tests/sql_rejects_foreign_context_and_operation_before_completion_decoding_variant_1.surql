UPDATE ONLY $task SET authority.context_key = 'another-context', request.input = { native: task_json_probe:opaque } RETURN NONE;
