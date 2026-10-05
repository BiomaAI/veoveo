DEFINE FUNCTION fn::kernel::identity::identity_labels_v1($tenant: record<tenant>, $context: record<work_context>, $principal: record<principal>) -> option<object> {
    LET $organization = SELECT * FROM ONLY $tenant;
    LET $current = SELECT * FROM ONLY $context;
    LET $actor = SELECT * FROM ONLY $principal;
    IF type::is_object($organization) {
        IF type::is_object($current) {
            IF type::is_object($actor) {
                IF $current.tenant = $tenant AND $actor.tenant = $tenant {
                    RETURN {
                        person: { id: $actor.id, display_name: $actor.display_name },
                        tenant_name: $organization.name,
                        work_context_title: $current.title,
                    };
                };
            };
        };
    };
    RETURN NONE;
} PERMISSIONS FULL;
