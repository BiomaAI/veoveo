UPDATE ONLY $task SET owner_context = task_policy_probe:owner, request.input = { native: task_json_probe:opaque } RETURN NONE;
