-- Chat bindings pin immutable registry revisions; current disable and audience override them.
DEFINE FUNCTION fn::agent_chat_revision($tenant: record<tenant>, $context: record<work_context>, $key: string, $digest: string, $new: bool) -> bool {
    LET $definition = array::first(SELECT * FROM agent_definition WHERE tenant = $tenant AND key = $key LIMIT 1);
    IF $definition = NONE OR $definition.disabled OR $context NOT IN $definition.audience
        OR ($new AND ($definition.status = 'archived' OR $definition.published.digest != $digest)) { RETURN false; };
    RETURN array::len(SELECT id FROM agent_definition_revision WHERE definition = $definition.id
        AND digest = $digest AND content.execution.kind = 'chat' LIMIT 1) = 1;
};

DEFINE TABLE workspace_agent_revision_receipt SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON workspace_agent_revision_receipt TYPE uuid;
DEFINE FIELD chat ON workspace_agent_revision_receipt TYPE record<workspace_chat>;
DEFINE FIELD actor ON workspace_agent_revision_receipt TYPE record<principal>;
DEFINE FIELD fingerprint ON workspace_agent_revision_receipt TYPE string;
DEFINE FIELD result ON workspace_agent_revision_receipt TYPE object FLEXIBLE;
DEFINE FIELD created_at ON workspace_agent_revision_receipt TYPE datetime DEFAULT time::now();
