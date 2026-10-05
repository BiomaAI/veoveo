UPDATE ONLY $task SET owner_context.tenant_key = 'inconsistent', request.input = { native: task_json_probe:opaque } RETURN NONE;
