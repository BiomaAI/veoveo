LET $keys = fn::kernel::identity::tenant_context_keys_v1($authority.tenant, $authority.work_context); RETURN {slug: $keys.tenant_key};
