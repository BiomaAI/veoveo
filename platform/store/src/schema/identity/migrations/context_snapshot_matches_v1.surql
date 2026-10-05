DEFINE FUNCTION fn::kernel::identity::context_snapshot_matches_v1($tenant: record<tenant>, $context: record<work_context>, $digest: string) -> bool {
    LET $current = SELECT * FROM ONLY $context;
    RETURN IF type::is_object($current) THEN
        $current.tenant = $tenant AND crypto::sha256(<string> [$current.policy_revision, $current.memberships, $current.output_policy]) = $digest
    ELSE false END;
} PERMISSIONS FULL;
