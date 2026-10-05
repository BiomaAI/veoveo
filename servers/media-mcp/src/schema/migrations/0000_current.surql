-- Current owner schema; schema definitions only.

DEFINE TABLE media_task_context TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE media_usage TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE FIELD OVERWRITE capability ON media_task_context TYPE record<artifact_write_capability> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE capability_expires_at ON media_task_context TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE capability_secret ON media_task_context TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON media_task_context TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON media_task_context TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON media_task_context TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON media_task_context TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON media_task_context TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX media_task_context_expiry ON media_task_context FIELDS capability_expires_at;

DEFINE INDEX media_task_context_task ON media_task_context FIELDS task UNIQUE;

DEFINE FIELD OVERWRITE amount ON media_usage TYPE none | float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE currency ON media_usage TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON media_usage TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE kind ON media_usage TYPE 'estimate' | 'actual' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE metadata ON media_usage TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;

DEFINE FIELD OVERWRITE model_id ON media_usage TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_job ON media_usage TYPE none | record<provider_job> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE quantity ON media_usage TYPE none | float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recorded_at ON media_usage TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_id ON media_usage TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON media_usage TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON media_usage TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE unit ON media_usage TYPE none | string PERMISSIONS FULL;

DEFINE INDEX media_usage_retention ON media_usage FIELDS recorded_at;

DEFINE INDEX media_usage_source_unique ON media_usage FIELDS task, kind, source_id UNIQUE;

DEFINE INDEX media_usage_task_time ON media_usage FIELDS task, recorded_at;

DEFINE TABLE media_task TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;
DEFINE FIELD OVERWRITE task ON media_task TYPE record<task> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE task_type ON media_task TYPE 'run' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at ON media_task TYPE datetime PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity ON media_task TYPE object READONLY PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.tenant ON media_task TYPE record<tenant> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.request ON media_task TYPE object FLEXIBLE READONLY PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context ON media_task TYPE object PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.principal_key ON media_task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.issuer ON media_task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.subject ON media_task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.profile ON media_task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.principal_kind ON media_task TYPE 'user' | 'service' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.tenant_key ON media_task TYPE none | null | string DEFAULT NULL PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.data_labels ON media_task TYPE array<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.data_labels.* ON media_task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority ON media_task TYPE object PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.work_context ON media_task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.tenant ON media_task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.policy_revision ON media_task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.membership ON media_task TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy ON media_task TYPE object PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy.owner ON media_task TYPE object PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy.owner.kind ON media_task TYPE 'principal' | 'group' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy.owner.id ON media_task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy.initial_grants.*.subject ON media_task TYPE object ASSERT object::keys($value) = ['id', 'kind'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy.initial_grants.*.subject.kind ON media_task TYPE 'principal' | 'group' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy.initial_grants.*.subject.id ON media_task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy.initial_grants ON media_task TYPE array<object> DEFAULT [] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy.initial_grants.* ON media_task TYPE object ASSERT object::keys($value) = ['level', 'subject'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy.initial_grants.*.level ON media_task TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy.classification ON media_task TYPE none | null | string DEFAULT NULL PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy.data_labels ON media_task TYPE array<string> DEFAULT [] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.output_policy.data_labels.* ON media_task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.provenance ON media_task TYPE object ASSERT ($value.mode = 'direct' AND type::is_string($value.initiator) AND $value.delegation_id = NONE) OR ($value.mode = 'delegated' AND type::is_string($value.initiator) AND type::is_string($value.delegation_id)) OR ($value.mode = 'automated' AND $value.initiator = NONE AND $value.delegation_id = NONE) PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.provenance.mode ON media_task TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.provenance.initiator ON media_task TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE identity.owner_context.authority.provenance.delegation_id ON media_task TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE dispatch ON media_task TYPE option<object> ASSERT IF type::is_object($value) THEN object::keys($value) = ['callback_digest'] AND $value.callback_digest != NONE ELSE $value = NONE END PERMISSIONS FULL;
DEFINE FIELD OVERWRITE dispatch.callback_digest ON media_task TYPE option<string> ASSERT $value = NONE OR $value = /^sha256:[0-9a-f]{64}$/ PERMISSIONS FULL;
DEFINE FIELD OVERWRITE association ON media_task TYPE option<object> ASSERT IF type::is_object($value) THEN object::keys($value) = ['job', 'prediction', 'provider'] AND $value.job != NONE AND $value.prediction != NONE AND $value.provider = 'media' ELSE $value = NONE END PERMISSIONS FULL;
DEFINE FIELD OVERWRITE association.job ON media_task TYPE option<record<provider_job>> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE association.prediction ON media_task TYPE option<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE association.provider ON media_task TYPE option<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE settlement ON media_task TYPE option<object> ASSERT IF type::is_object($value) THEN $value.status != NONE AND $value.completed_at != NONE AND (IF $value.status = 'succeeded' THEN { LET $association: object = $this.association; $value.expected_result != NONE AND $value.prediction != NONE AND $value.prediction = $association.prediction } ELSE $value.expected_result = NONE AND $value.prediction = NONE END) ELSE $value = NONE END PERMISSIONS FULL;
DEFINE FIELD OVERWRITE settlement.status ON media_task TYPE option<'succeeded' | 'failed' | 'cancelled'> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE settlement.completed_at ON media_task TYPE option<datetime> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE settlement.prediction ON media_task TYPE option<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE settlement.expected_result ON media_task TYPE option<object> FLEXIBLE PERMISSIONS FULL;
DEFINE INDEX media_task_identity ON media_task FIELDS task UNIQUE;
DEFINE INDEX media_task_owner ON media_task FIELDS identity.tenant, identity.owner_context.principal_key, created_at;
DEFINE INDEX media_task_provider ON media_task FIELDS association.job UNIQUE;
DEFINE INDEX media_task_prediction ON media_task FIELDS identity.tenant, association.prediction UNIQUE;
