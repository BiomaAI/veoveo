DEFINE FUNCTION fn::kernel::identity::identity_enabled_v1($tenant: record<tenant>, $context: record<work_context>, $principal: record<principal>) -> bool {
    LET $organization = SELECT * FROM ONLY $tenant;
    LET $actor = SELECT * FROM ONLY $principal;
    LET $current = SELECT * FROM ONLY $context;
    RETURN IF type::is_object($organization) THEN
      IF type::is_object($actor) THEN
        IF type::is_object($current) THEN
          $organization.enabled = true AND $actor.enabled = true AND $current.tenant = $tenant
        ELSE false END
      ELSE false END
    ELSE false END;
} PERMISSIONS FULL;
