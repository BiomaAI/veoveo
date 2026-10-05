DEFINE FUNCTION fn::kernel::identity::tenant_current_v1($enterprise: record<enterprise>, $tenant: record<tenant>) -> option<object> {
    LET $organization = SELECT * FROM ONLY $enterprise;
    LET $current = SELECT * FROM ONLY $tenant;
    RETURN IF type::is_object($organization) THEN
      IF type::is_object($current) THEN
        IF $organization.enabled = true AND $current.enabled = true AND $current.enterprise = $enterprise
        THEN { slug: $current.slug } ELSE NONE END
      ELSE NONE END
    ELSE NONE END;
} PERMISSIONS FULL;
