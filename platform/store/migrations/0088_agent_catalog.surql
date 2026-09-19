-- Durable agent authoring; executable revisions never contain provider credentials.
DEFINE TABLE agent_definition SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON agent_definition TYPE uuid;
DEFINE FIELD tenant ON agent_definition TYPE record<tenant>;
DEFINE FIELD work_context ON agent_definition TYPE record<work_context>;
DEFINE FIELD owner ON agent_definition TYPE record<principal>;
DEFINE FIELD key ON agent_definition TYPE string;
DEFINE FIELD name ON agent_definition TYPE string;
DEFINE FIELD description ON agent_definition TYPE string;
DEFINE FIELD revision ON agent_definition TYPE int ASSERT $value > 0;
DEFINE FIELD status ON agent_definition TYPE 'enabled' | 'disabled' | 'archived';
DEFINE FIELD disabled ON agent_definition TYPE bool DEFAULT false;
DEFINE FIELD audience ON agent_definition TYPE array<record<work_context>>;
DEFINE FIELD draft ON agent_definition TYPE object FLEXIBLE;
DEFINE FIELD draft_digest ON agent_definition TYPE string;
DEFINE FIELD published ON agent_definition TYPE option<record<agent_definition_revision>>;
DEFINE FIELD created_at ON agent_definition TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON agent_definition TYPE datetime DEFAULT time::now();
DEFINE INDEX agent_definition_key ON agent_definition FIELDS tenant, key UNIQUE;
DEFINE INDEX agent_definition_editor ON agent_definition FIELDS tenant, work_context, key;
DEFINE INDEX agent_definition_audience ON agent_definition FIELDS tenant, audience, key;

DEFINE TABLE agent_definition_revision SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON agent_definition_revision TYPE uuid DEFAULT rand::uuid::v7();
DEFINE FIELD definition ON agent_definition_revision TYPE record<agent_definition>
  READONLY;
DEFINE FIELD digest ON agent_definition_revision TYPE string READONLY;
DEFINE FIELD content ON agent_definition_revision TYPE object FLEXIBLE  READONLY;
DEFINE FIELD created_by ON agent_definition_revision TYPE record<principal>
  READONLY;
DEFINE FIELD created_at ON agent_definition_revision TYPE datetime
  DEFAULT time::now()
  READONLY;
DEFINE INDEX agent_revision_digest ON agent_definition_revision FIELDS definition, digest
  UNIQUE;
DEFINE INDEX agent_revision_history ON agent_definition_revision FIELDS definition, created_at;

DEFINE TABLE agent_catalog_capacity SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON agent_catalog_capacity TYPE uuid;
DEFINE FIELD tenant ON agent_catalog_capacity TYPE record<tenant>;
DEFINE FIELD work_context ON agent_catalog_capacity TYPE record<work_context>;
DEFINE FIELD definitions ON agent_catalog_capacity TYPE int
  DEFAULT 0
  ASSERT $value >= 0;

DEFINE TABLE agent_definition_receipt SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON agent_definition_receipt TYPE uuid;
DEFINE FIELD tenant ON agent_definition_receipt TYPE record<tenant>;
DEFINE FIELD work_context ON agent_definition_receipt TYPE record<work_context>;
DEFINE FIELD actor ON agent_definition_receipt TYPE record<principal>;
DEFINE FIELD request_id ON agent_definition_receipt TYPE uuid;
DEFINE FIELD fingerprint ON agent_definition_receipt TYPE string;
DEFINE FIELD result ON agent_definition_receipt TYPE object FLEXIBLE;
DEFINE FIELD created_at ON agent_definition_receipt TYPE datetime
  DEFAULT time::now();

DEFINE FUNCTION fn::agent_catalog_authority($authority: object) -> bool {
  LET $context = SELECT * FROM ONLY $authority.work_context;
  LET $principal = SELECT * FROM ONLY $authority.principal;
  RETURN $context != NONE
  AND $principal != NONE
  AND $principal.enabled = true
  AND $principal.tenant = $authority.tenant
  AND $authority.tenant.enabled = true
  AND $context.tenant = $authority.tenant
  AND crypto::sha256(
    <string> [
      $context.policy_revision,
      $context.memberships,
      $context.output_policy,
    ]
  ) = $authority.context_digest;
};

DEFINE FUNCTION fn::agent_definition_editor($authority: object, $definition: any) -> bool {
  RETURN $definition != NONE
  AND $definition.tenant = $authority.tenant
  AND $definition.work_context = $authority.work_context
  AND (
    $definition.owner = $authority.principal
    OR $authority.manage_context
  );
};
