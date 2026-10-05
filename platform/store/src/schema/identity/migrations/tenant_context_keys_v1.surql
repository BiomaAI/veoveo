DEFINE FUNCTION fn::kernel::identity::tenant_context_keys_v1($tenant: record<tenant>, $context: record<work_context>) -> option<object> {
    LET $organization = SELECT * FROM ONLY $tenant;
    LET $current = SELECT * FROM ONLY $context;
    RETURN IF type::is_object($organization) THEN
        IF type::is_object($current) THEN
            IF $current.tenant = $tenant THEN { tenant_key: $organization.slug, context_key: $current.context_key }
            ELSE NONE END
        ELSE NONE END
    ELSE NONE END;
} PERMISSIONS FULL;
