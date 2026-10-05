UPDATE ONLY $task SET task_type = 'other-operation', request.input = { native: task_json_probe:opaque } RETURN NONE;
