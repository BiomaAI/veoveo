RETURN fn::kernel::identity::tenant_current_v1($enterprise, $tenant);
RETURN fn::kernel::identity::principal_current_v1($tenant, $principal);
RETURN fn::kernel::identity::tenant_matches_v1($enterprise, $tenant, $slug);
