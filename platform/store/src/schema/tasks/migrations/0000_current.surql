-- Current owner schema; schema definitions only.

DEFINE TABLE domain_usage TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE provider_event TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE provider_job TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE task TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE task_idempotency TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE task_input TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE task_produced_artifact TYPE RELATION IN task OUT artifact_occurrence ENFORCED SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE task_used_artifact TYPE RELATION IN task OUT artifact_occurrence ENFORCED SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE FIELD OVERWRITE amount ON domain_usage TYPE none | float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE currency ON domain_usage TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON domain_usage TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE kind ON domain_usage TYPE 'estimate' | 'actual' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE metadata ON domain_usage TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;

DEFINE FIELD OVERWRITE model_id ON domain_usage TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_job_id ON domain_usage TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE quantity ON domain_usage TYPE none | float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recorded_at ON domain_usage TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE server ON domain_usage TYPE record<mcp_server> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_id ON domain_usage TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON domain_usage TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON domain_usage TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE unit ON domain_usage TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON domain_usage TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX domain_usage_server_task ON domain_usage FIELDS server, task;

DEFINE INDEX domain_usage_task_time ON domain_usage FIELDS task, recorded_at;

DEFINE INDEX domain_usage_tenant_time ON domain_usage FIELDS tenant, recorded_at;

DEFINE FIELD OVERWRITE event_id ON provider_event TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON provider_event TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE payload ON provider_event TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE processed_at ON provider_event TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE processing_error ON provider_event TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider ON provider_event TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_job ON provider_event TYPE record<provider_job> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE received_at ON provider_event TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE signing_key_id ON provider_event TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON provider_event TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX provider_event_job ON provider_event FIELDS provider_job, received_at;

DEFINE INDEX provider_event_unique ON provider_event FIELDS tenant, provider, event_id UNIQUE;

DEFINE INDEX provider_event_unprocessed ON provider_event FIELDS processed_at, received_at;

DEFINE FIELD OVERWRITE completed_at ON provider_job TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE external_job_id ON provider_job TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON provider_job TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider ON provider_job TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_payload ON provider_job TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON provider_job TYPE 'submitted' | 'waiting' | 'succeeded' | 'failed' | 'cancel_requested' | 'cancelled' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE submitted_at ON provider_job TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON provider_job TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON provider_job TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON provider_job TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE cancellation_receipt ON provider_job TYPE option<object> FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE terminal_event ON provider_job TYPE option<record<provider_event>> REFERENCE ON DELETE REJECT PERMISSIONS FULL;

DEFINE FIELD OVERWRITE observed_at ON provider_job TYPE option<datetime> ASSERT ($value = NONE) = ($this.terminal_event = NONE) PERMISSIONS FULL;

DEFINE INDEX provider_job_external_unique ON provider_job FIELDS tenant, provider, external_job_id UNIQUE;

DEFINE INDEX provider_job_state_updated ON provider_job FIELDS state, updated_at;

DEFINE INDEX provider_job_task ON provider_job FIELDS task;

DEFINE FIELD OVERWRITE authority ON task TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON task TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON task TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON task TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON task TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON task TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON task TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON task TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON task TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON task TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON task TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON task TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON task TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON task TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON task TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON task TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON task TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE cancel_requested_at ON task TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE completed_at ON task TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON task TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE created_at_exact ON task TYPE string VALUE type::string($this.created_at) PERMISSIONS FULL;

DEFINE FIELD OVERWRITE delegation_id ON task TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE error ON task TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON task TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE idempotency_key ON task TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE initiator ON task TYPE none | record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE invocation_mode ON task TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_expires_at ON task TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_owner ON task TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON task TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE policy_revision ON task TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile ON task TYPE record<profile> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE progress ON task TYPE float DEFAULT 0f ASSERT $value >= 0f AND $value <= 1f PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recovery_class ON task TYPE 'resume' | 'webhook_wait' | 'provider_wait' | 'interrupted_indeterminate' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_context ON task TYPE object PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.principal_key ON task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.issuer ON task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.subject ON task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.profile ON task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.principal_kind ON task TYPE 'user' | 'service' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.tenant_key ON task TYPE none | null | string DEFAULT NULL PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.data_labels ON task TYPE array<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.data_labels.* ON task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority ON task TYPE object PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.work_context ON task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.tenant ON task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.policy_revision ON task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.membership ON task TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy ON task TYPE object PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy.owner ON task TYPE object PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy.owner.kind ON task TYPE 'principal' | 'group' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy.owner.id ON task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy.initial_grants.*.subject ON task TYPE object ASSERT object::keys($value) = ['id', 'kind'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy.initial_grants.*.subject.kind ON task TYPE 'principal' | 'group' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy.initial_grants.*.subject.id ON task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy.initial_grants ON task TYPE array<object> DEFAULT [] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy.initial_grants.* ON task TYPE object ASSERT object::keys($value) = ['level', 'subject'] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy.initial_grants.*.level ON task TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy.classification ON task TYPE none | null | string DEFAULT NULL PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy.data_labels ON task TYPE array<string> DEFAULT [] PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.output_policy.data_labels.* ON task TYPE string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.provenance ON task TYPE object ASSERT ($value.mode = 'direct' AND type::is_string($value.initiator) AND $value.delegation_id = NONE) OR ($value.mode = 'delegated' AND type::is_string($value.initiator) AND type::is_string($value.delegation_id)) OR ($value.mode = 'automated' AND $value.initiator = NONE AND $value.delegation_id = NONE) PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.provenance.mode ON task TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.provenance.initiator ON task TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE owner_context.authority.provenance.delegation_id ON task TYPE none | string PERMISSIONS FULL;
DEFINE FIELD OVERWRITE request ON task TYPE object PERMISSIONS FULL;
DEFINE FIELD OVERWRITE request.input ON task TYPE null | bool | number | string | object | array FLEXIBLE ASSERT $value != NONE PERMISSIONS FULL;
DEFINE FIELD OVERWRITE request.status_message ON task TYPE none | null | string DEFAULT NULL PERMISSIONS FULL;
DEFINE FIELD OVERWRITE request.ttl_ms ON task TYPE none | null | int | decimal DEFAULT NULL ASSERT $value = NONE OR $value = NULL OR ($value >= 0 AND $value <= 18446744073709551615dec AND $value = math::floor($value)) PERMISSIONS FULL;
DEFINE FIELD OVERWRITE request.poll_interval_ms ON task TYPE none | null | int | decimal DEFAULT NULL ASSERT $value = NONE OR $value = NULL OR ($value >= 0 AND $value <= 18446744073709551615dec AND $value = math::floor($value)) PERMISSIONS FULL;


DEFINE FIELD OVERWRITE result ON task TYPE none | object FLEXIBLE ASSERT $value = NONE OR object::keys($value) = ['payload'] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE result_uri ON task TYPE option<string> ASSERT $value = NONE OR ($this.status = 'succeeded' AND $this.result != NONE) PERMISSIONS FULL;
DEFINE FIELD OVERWRITE result_artifact ON task TYPE none | record<artifact_occurrence> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE retention_expires_at ON task TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE retention_pins ON task TYPE array<string> DEFAULT [] VALUE $value.distinct() ASSERT $value.all(|$pin: any| string::len($pin) > 0 AND string::len($pin) <= 256) PERMISSIONS FULL;

DEFINE FIELD OVERWRITE retention_pins.* ON task TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE search_text ON task TYPE string DEFAULT '' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE server ON task TYPE record<mcp_server> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE started_at ON task TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE status ON task TYPE 'queued' | 'running' | 'waiting' | 'succeeded' | 'failed' | 'cancel_requested' | 'cancelled' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_type ON task TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON task TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON task TYPE datetime DEFAULT time::now() PERMISSIONS FULL;
DEFINE FIELD OVERWRITE updated_at_exact ON task TYPE string VALUE type::string($this.updated_at) PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON task TYPE record<work_context> PERMISSIONS FULL;




DEFINE INDEX task_context_status_created ON task FIELDS tenant, work_context, status, created_at;

DEFINE INDEX task_count ON task COUNT;

DEFINE INDEX task_domain_owner_page ON task FIELDS server, tenant, owner, profile, work_context, created_at, id;

DEFINE INDEX task_owner_status_created ON task FIELDS tenant, owner, status, created_at;

DEFINE INDEX task_queued_count ON task COUNT WHERE status = 'queued';

DEFINE INDEX task_retention ON task FIELDS retention_expires_at;

DEFINE INDEX task_running_count ON task COUNT WHERE status = 'running';

DEFINE INDEX task_search ON task FIELDS search_text FULLTEXT ANALYZER platform_search BM25(1.2,0.75);

DEFINE INDEX task_status_lease ON task FIELDS status, lease_expires_at;

DEFINE INDEX task_tenant_created ON task FIELDS tenant, created_at;


DEFINE INDEX task_waiting_count ON task COUNT WHERE status = 'waiting';

DEFINE FIELD OVERWRITE created_at ON task_idempotency TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON task_idempotency TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE key ON task_idempotency TYPE string ASSERT string::len($value) > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON task_idempotency TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE server ON task_idempotency TYPE record<mcp_server> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON task_idempotency TYPE record<task> REFERENCE ON DELETE CASCADE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON task_idempotency TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX task_idempotency_owner_server ON task_idempotency FIELDS tenant, owner, server, created_at;

DEFINE INDEX task_idempotency_task ON task_idempotency FIELDS task UNIQUE;

DEFINE FIELD OVERWRITE created_at ON task_input TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON task_input TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE request ON task_input TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE request_key ON task_input TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 256 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE responded_at ON task_input TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE response ON task_input TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON task_input TYPE record<task> REFERENCE ON DELETE CASCADE PERMISSIONS FULL;

DEFINE INDEX task_input_outstanding ON task_input FIELDS task, response, created_at;

DEFINE INDEX task_input_task_key ON task_input FIELDS task, request_key UNIQUE;

DEFINE FIELD OVERWRITE created_at ON task_produced_artifact TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON task_produced_artifact TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE in ON task_produced_artifact TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON task_produced_artifact TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE out ON task_produced_artifact TYPE record<artifact_occurrence> PERMISSIONS FULL;

DEFINE INDEX artifact_producer ON task_produced_artifact FIELDS out;

DEFINE INDEX task_produced_artifact_unique ON task_produced_artifact FIELDS in, name UNIQUE;

DEFINE FIELD OVERWRITE created_at ON task_used_artifact TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON task_used_artifact TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE in ON task_used_artifact TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON task_used_artifact TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE out ON task_used_artifact TYPE record<artifact_occurrence> PERMISSIONS FULL;

DEFINE INDEX artifact_consumers ON task_used_artifact FIELDS out;

DEFINE INDEX task_used_artifact_unique ON task_used_artifact FIELDS in, out, name UNIQUE;

DEFINE INDEX task_result_uri ON task FIELDS result_uri;
