UPDATE ONLY $id SET owner_context.authority = task_policy_probe:authority, request.input = { native: task_json_probe:opaque } RETURN NONE;
