DEFINE FUNCTION fn::kernel::identity::principal_current_v1($tenant: record<tenant>, $principal: record<principal>) -> option<object> {
    LET $current = SELECT * FROM ONLY $principal;
    RETURN IF type::is_object($current) THEN
      IF $current.enabled = true AND $current.tenant = $tenant THEN
        { kind: $current.kind, issuer: $current.issuer, subject: $current.subject }
      ELSE NONE END
    ELSE NONE END;
} PERMISSIONS FULL;
