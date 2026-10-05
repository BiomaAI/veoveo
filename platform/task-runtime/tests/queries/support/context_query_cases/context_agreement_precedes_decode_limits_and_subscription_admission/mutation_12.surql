UPDATE ONLY $id SET owner_context.authority.tenant = 'another-tenant', request.input = { native: task_json_probe:opaque } RETURN NONE;
