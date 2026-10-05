RETURN [
    fn::kernel::identity::tenant_context_keys_v1($tenant, $context) != NONE,
    fn::kernel::identity::tenant_context_keys_v1($foreign_tenant, $context) = NONE,
    fn::kernel::identity::tenant_context_keys_v1($tenant, $missing_context) = NONE,
    fn::kernel::identity::tenant_context_keys_v1($missing_tenant, $context) = NONE
];
