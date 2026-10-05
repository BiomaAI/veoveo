DEFINE FUNCTION fn::kernel::identity::actor_admitted_v1($tenant: record<tenant>, $context: record<work_context>, $principal: record<principal>, $context_digest: string, $require_human: bool) -> bool {
    LET $organization = SELECT * FROM ONLY $tenant;
    LET $current = SELECT * FROM ONLY $context;
    LET $actor = SELECT * FROM ONLY $principal;
    RETURN IF type::is_object($organization) THEN
      IF type::is_object($current) THEN
        IF type::is_object($actor) THEN
        $actor.enabled = true AND (!$require_human OR $actor.kind = 'user') AND
        $actor.tenant = $tenant AND $organization.enabled = true AND $current.tenant = $tenant AND
        crypto::sha256(<string> [$current.policy_revision, $current.memberships, $current.output_policy]) = $context_digest
        ELSE false END
      ELSE false END
    ELSE false END;
} PERMISSIONS FULL;
