-- Retained identity integrity does not grant current admission authority.
DEFINE FUNCTION fn::kernel::identity::tenant_matches_v1(
    $enterprise: record<enterprise>, $tenant: record<tenant>, $slug: string
) -> bool {
    LET $organization = SELECT * FROM ONLY $enterprise;
    LET $current = SELECT * FROM ONLY $tenant;
    RETURN IF type::is_object($organization) THEN
      IF type::is_object($current) THEN
        $current.enterprise = $enterprise AND $current.slug = $slug
      ELSE false END
    ELSE false END;
} PERMISSIONS FULL;
