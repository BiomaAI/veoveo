-- Current owner schema; schema definitions only.

DEFINE TABLE agent TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE TABLE agent_catalog_capacity TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE agent_definition TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE agent_definition_receipt TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE agent_definition_revision TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE agent_episode TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE agent_input_request TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;


DEFINE TABLE agent_task TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE managed_agent TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE managed_agent_capacity TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE managed_agent_operation TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE wake TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d INCLUDE ORIGINAL PERMISSIONS NONE;

DEFINE FUNCTION fn::agent_catalog_authority($authority: object) -> bool {
    LET $tenant: record<tenant> = $authority.tenant;
    LET $context: record<work_context> = $authority.work_context;
    LET $principal: record<principal> = $authority.principal;
    LET $digest: string = $authority.context_digest;
    RETURN fn::kernel::identity::actor_admitted_v1($tenant, $context, $principal, $digest, false);
} PERMISSIONS FULL;

DEFINE FUNCTION fn::agent_chat_revision($tenant: record<tenant>, $context: record<work_context>, $key: string, $digest: string, $new: bool) -> bool {
    LET $definition = array::first(SELECT * FROM agent_definition WHERE tenant = $tenant AND key = $key LIMIT 1);
    RETURN IF type::is_object($definition) THEN {
        IF $definition.disabled OR $context NOTINSIDE $definition.audience { RETURN false; };
        IF $new {
            IF $definition.status = 'archived' { RETURN false; };
            IF $definition.published = NONE { RETURN false; };
            LET $published_id: record<agent_definition_revision> = $definition.published;
            LET $published = SELECT * FROM ONLY $published_id;
            IF type::is_object($published) THEN {
                IF $published.digest != $digest { RETURN false; };
            } ELSE { RETURN false; } END;
        };
        LET $revision = array::first(SELECT content FROM agent_definition_revision WHERE definition = $definition.id AND digest = $digest LIMIT 1);
        RETURN IF type::is_object($revision) THEN {
            LET $content = $revision.content;
            RETURN IF type::is_object($content) THEN {
                LET $execution = $content.execution;
                RETURN IF type::is_object($execution) THEN $execution.kind = 'chat' ELSE false END;
            } ELSE false END;
        } ELSE false END;
    } ELSE false END;
} PERMISSIONS FULL;

DEFINE FUNCTION fn::agent_consume_results($agent: record<agent>, $wakes: array<record<wake>>, $episode: record<agent_episode>, $now: datetime) -> bool {
    LET $deliveries = SELECT task_id, retention_pin FROM agent_task WHERE agent = $agent AND result_wake INSIDE $wakes AND consumed_by_episode = NONE AND retention_pin_active = true;
    FOR $delivery IN $deliveries {
        IF type::is_object($delivery) THEN {
            LET $route_id: record<gateway_task_route> = type::record('gateway_task_route', $delivery.task_id);
            LET $route = fn::kernel::gateway::task_retention_route_v1($route_id);
            IF type::is_object($route) THEN {
                IF $route.source_task != NONE {
                    LET $source_task: record<task> = $route.source_task;
                    LET $server: record<mcp_server> = $route.server;
                    LET $pin: string = $delivery.retention_pin;
                    fn::kernel::tasks::release_retention_v1($source_task, $server, $pin);
                };
            } ELSE { THROW 'gateway task route missing during retention release'; } END;
        } ELSE { THROW 'gateway task route missing during retention release'; } END;
    };
    UPDATE agent_task SET consumed_by_episode = $episode, retention_pin_active = false, lease_owner = NONE, lease_expires_at = NONE, updated_at = $now, revision += 1 WHERE agent = $agent AND result_wake INSIDE $wakes AND consumed_by_episode = NONE RETURN NONE;
    RETURN true;
} PERMISSIONS FULL;

DEFINE FUNCTION fn::agent_definition_editor($authority: object, $definition: any) -> bool {
    RETURN IF type::is_object($definition) THEN $definition.tenant = $authority.tenant AND $definition.work_context = $authority.work_context AND ($definition.owner = $authority.principal OR $authority.manage_context) ELSE false END;
} PERMISSIONS FULL;

DEFINE FUNCTION fn::managed_agent_claim($claim: object) -> bool {
    LET $operation_id: record<managed_agent_operation> = $claim.operation;
    LET $operation = SELECT * FROM ONLY $operation_id;
    RETURN IF type::is_object($operation) THEN {
        LET $instance_id: record<managed_agent> = $operation.instance;
        LET $instance = SELECT * FROM ONLY $instance_id;
        RETURN IF type::is_object($instance) THEN
            $operation.instance = $claim.instance AND $operation.generation = $claim.generation AND
            $operation.lease_owner = $claim.owner AND $operation.lease_fence = $claim.fence AND
            $operation.lease_expires_at > time::now() AND $instance.operation = $claim.operation AND
            $instance.generation = $claim.generation
        ELSE false END;
    } ELSE false END;
} PERMISSIONS FULL;

DEFINE FUNCTION fn::managed_agent_editor($authority: object, $instance: any) -> bool {
    RETURN IF type::is_object($instance) THEN $instance.tenant = $authority.tenant AND $instance.work_context = $authority.work_context AND ($instance.owner = $authority.principal OR $authority.manage_context) ELSE false END;
} PERMISSIONS FULL;

DEFINE FUNCTION fn::managed_agent_enabled($id: record<managed_agent>) -> bool {
    LET $instance = SELECT * FROM ONLY $id;
    RETURN IF type::is_object($instance) THEN {
        IF $instance.desired = 'archived' { RETURN false; };
        LET $tenant: record<tenant> = $instance.tenant;
        LET $context: record<work_context> = $instance.work_context;
        LET $principal: record<principal> = $instance.principal;
        IF !fn::kernel::identity::identity_enabled_v1($tenant, $context, $principal) { RETURN false; };
        LET $definition_id: record<agent_definition> = $instance.definition;
        LET $definition = SELECT * FROM ONLY $definition_id;
        RETURN IF type::is_object($definition) THEN
            $definition.tenant = $instance.tenant AND $definition.disabled = false AND $instance.work_context INSIDE $definition.audience
        ELSE false END;
    } ELSE false END;
} PERMISSIONS FULL;

DEFINE FIELD OVERWRITE agent_key ON agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority ON agent TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.classification ON agent TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.context_key ON agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels ON agent TYPE array<string> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.data_labels.* ON agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.delegation_id ON agent TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants ON agent TYPE array<object> DEFAULT [] PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.* ON agent TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.permission ON agent TYPE 'read' | 'write' | 'admin' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_key ON agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initial_grants.*.subject_kind ON agent TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.initiator_key ON agent TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.invocation_mode ON agent TYPE 'direct' | 'delegated' | 'automated' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.membership ON agent TYPE 'viewer' | 'contributor' | 'custodian' | 'owner' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_key ON agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.owner_kind ON agent TYPE 'principal' | 'group' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE authority.policy_revision ON agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON agent TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE display_name ON agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE fence ON agent TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE heartbeat_at ON agent TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON agent TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE last_episode ON agent TYPE none | record<agent_episode> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_expires_at ON agent TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_owner ON agent TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE managed_ready ON agent TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE manifest ON agent TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE memory_database ON agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE next_episode_sequence ON agent TYPE int DEFAULT 1 ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE policy_revision ON agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile ON agent TYPE record<profile> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON agent TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON agent TYPE 'idle' | 'running' | 'waiting' | 'disabled' | 'failed' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON agent TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON agent TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON agent TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX agent_context_state ON agent FIELDS tenant, work_context, state;

DEFINE INDEX agent_lease_expiry ON agent FIELDS lease_expires_at;

DEFINE INDEX agent_tenant_key_unique ON agent FIELDS tenant, agent_key UNIQUE;

DEFINE INDEX agent_tenant_state ON agent FIELDS tenant, state;

DEFINE FIELD OVERWRITE definitions ON agent_catalog_capacity TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON agent_catalog_capacity TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON agent_catalog_capacity TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON agent_catalog_capacity TYPE record<work_context> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE audience ON agent_definition TYPE array<record<work_context>> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE audience.* ON agent_definition TYPE record<work_context> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON agent_definition TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE description ON agent_definition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE disabled ON agent_definition TYPE bool DEFAULT false PERMISSIONS FULL;

DEFINE FIELD OVERWRITE draft_model ON agent_definition TYPE object ASSERT IF type::is_object($value) THEN object::keys($value) ALLINSIDE ['id', 'revision'] AND $value.id != NONE AND $value.revision != NONE ELSE false END;
DEFINE FIELD OVERWRITE draft_model.id ON agent_definition TYPE string;
DEFINE FIELD OVERWRITE draft_model.revision ON agent_definition TYPE string;
DEFINE FIELD OVERWRITE draft_tools ON agent_definition TYPE array<string>;

DEFINE FIELD OVERWRITE draft_execution ON agent_definition TYPE object ASSERT IF type::is_object($value) THEN object::keys($value) ALLINSIDE ['kind', 'template', 'template_revision', 'parameters', 'resource_subscriptions'] AND (($value.kind = 'chat' AND $value.template = NONE AND $value.template_revision = NONE AND $value.parameters = NONE AND $value.resource_subscriptions = NONE) OR ($value.kind = 'managed' AND $value.template != NONE AND $value.template_revision != NONE AND $value.parameters != NONE AND $value.resource_subscriptions != NONE)) ELSE false END;
DEFINE FIELD OVERWRITE draft_execution.kind ON agent_definition TYPE 'chat' | 'managed';
DEFINE FIELD OVERWRITE draft_execution.template ON agent_definition TYPE option<string>;
DEFINE FIELD OVERWRITE draft_execution.template_revision ON agent_definition TYPE option<string>;
DEFINE FIELD OVERWRITE draft_execution.parameters ON agent_definition TYPE option<object> FLEXIBLE;
DEFINE FIELD OVERWRITE draft_execution.resource_subscriptions ON agent_definition TYPE option<array<string>>;

DEFINE FIELD OVERWRITE draft ON agent_definition TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE draft_digest ON agent_definition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON agent_definition TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE key ON agent_definition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON agent_definition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON agent_definition TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE published ON agent_definition TYPE none | record<agent_definition_revision> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON agent_definition TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE status ON agent_definition TYPE 'enabled' | 'disabled' | 'archived' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON agent_definition TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON agent_definition TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON agent_definition TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX agent_definition_audience ON agent_definition FIELDS tenant, audience, key;

DEFINE INDEX agent_definition_editor ON agent_definition FIELDS tenant, work_context, key;

DEFINE INDEX agent_definition_key ON agent_definition FIELDS tenant, key UNIQUE;

DEFINE FIELD OVERWRITE actor ON agent_definition_receipt TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON agent_definition_receipt TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE fingerprint ON agent_definition_receipt TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON agent_definition_receipt TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE request_id ON agent_definition_receipt TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE result ON agent_definition_receipt TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON agent_definition_receipt TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON agent_definition_receipt TYPE record<work_context> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE model ON agent_definition_revision TYPE object ASSERT IF type::is_object($value) THEN object::keys($value) ALLINSIDE ['id', 'revision'] AND $value.id != NONE AND $value.revision != NONE ELSE false END;
DEFINE FIELD OVERWRITE model.id ON agent_definition_revision TYPE string;
DEFINE FIELD OVERWRITE model.revision ON agent_definition_revision TYPE string;
DEFINE FIELD OVERWRITE tools ON agent_definition_revision TYPE array<string>;

DEFINE FIELD OVERWRITE execution ON agent_definition_revision TYPE object ASSERT IF type::is_object($value) THEN object::keys($value) ALLINSIDE ['kind', 'template', 'template_revision', 'parameters', 'resource_subscriptions'] AND (($value.kind = 'chat' AND $value.template = NONE AND $value.template_revision = NONE AND $value.parameters = NONE AND $value.resource_subscriptions = NONE) OR ($value.kind = 'managed' AND $value.template != NONE AND $value.template_revision != NONE AND $value.parameters != NONE AND $value.resource_subscriptions != NONE)) ELSE false END;
DEFINE FIELD OVERWRITE execution.kind ON agent_definition_revision TYPE 'chat' | 'managed';
DEFINE FIELD OVERWRITE execution.template ON agent_definition_revision TYPE option<string>;
DEFINE FIELD OVERWRITE execution.template_revision ON agent_definition_revision TYPE option<string>;
DEFINE FIELD OVERWRITE execution.parameters ON agent_definition_revision TYPE option<object> FLEXIBLE;
DEFINE FIELD OVERWRITE execution.resource_subscriptions ON agent_definition_revision TYPE option<array<string>>;

DEFINE FIELD OVERWRITE template_revision ON agent_definition_revision TYPE option<string> VALUE { LET $execution: object = $this.execution; RETURN $execution.template_revision; };

DEFINE FIELD OVERWRITE content ON agent_definition_revision TYPE object FLEXIBLE READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON agent_definition_revision TYPE datetime DEFAULT time::now() READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by ON agent_definition_revision TYPE record<principal> READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE definition ON agent_definition_revision TYPE record<agent_definition> READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE digest ON agent_definition_revision TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON agent_definition_revision TYPE uuid DEFAULT `rand`::uuid::v7() PERMISSIONS FULL;

DEFINE INDEX agent_revision_digest ON agent_definition_revision FIELDS definition, digest UNIQUE;

DEFINE INDEX agent_revision_history ON agent_definition_revision FIELDS definition, created_at;

DEFINE FIELD OVERWRITE agent ON agent_episode TYPE record<agent> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE completion_calls ON agent_episode TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE error ON agent_episode TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE final_output ON agent_episode TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE finished_at ON agent_episode TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON agent_episode TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE input_tokens ON agent_episode TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE managed ON agent_episode TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE output_tokens ON agent_episode TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE retention_pin ON agent_episode TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON agent_episode TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE `sequence` ON agent_episode TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE started_at ON agent_episode TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON agent_episode TYPE 'running' | 'completed' | 'budget_terminated' | 'failed' | 'crashed' | 'stopped' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE summary ON agent_episode TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON agent_episode TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tool_calls ON agent_episode TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE wake_note ON agent_episode TYPE string PERMISSIONS FULL;

DEFINE INDEX agent_episode_sequence_unique ON agent_episode FIELDS agent, `sequence` UNIQUE;

DEFINE INDEX agent_episode_state_started ON agent_episode FIELDS agent, state, started_at;

DEFINE FIELD OVERWRITE agent ON agent_input_request TYPE record<agent> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE answer ON agent_input_request TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE answered_at ON agent_input_request TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE answered_by ON agent_input_request TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON agent_input_request TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE message ON agent_input_request TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE related_task ON agent_input_request TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE requested_at ON agent_input_request TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE requested_schema ON agent_input_request TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON agent_input_request TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON agent_input_request TYPE 'pending' | 'answered' | 'declined' | 'cancelled' DEFAULT 'pending' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON agent_input_request TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX agent_input_request_state ON agent_input_request FIELDS agent, state, requested_at;








DEFINE FIELD OVERWRITE agent ON agent_task TYPE record<agent> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE attempt_count ON agent_task TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE consumed_by_episode ON agent_task TYPE none | record<agent_episode> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON agent_task TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE descriptor ON agent_task TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE descriptor_complete ON agent_task TYPE bool DEFAULT false PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON agent_task TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE last_error ON agent_task TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_expires_at ON agent_task TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_owner ON agent_task TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE next_retry_at ON agent_task TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE resolved_at ON agent_task TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE result ON agent_task TYPE none | object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE result_is_error ON agent_task TYPE bool DEFAULT false PERMISSIONS FULL;

DEFINE FIELD OVERWRITE result_wake ON agent_task TYPE none | record<wake> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE retention_pin ON agent_task TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE retention_pin_active ON agent_task TYPE bool DEFAULT true PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON agent_task TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE started_by_episode ON agent_task TYPE record<agent_episode> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON agent_task TYPE 'pending' | 'watching' | 'resolved' | 'failed' | 'cancelled' DEFAULT 'pending' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_id ON agent_task TYPE string ASSERT string::len($value) > 0 AND string::len($value) <= 128 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON agent_task TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tool_name ON agent_task TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON agent_task TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX agent_task_available ON agent_task FIELDS agent, state, next_retry_at;

DEFINE INDEX agent_task_lease ON agent_task FIELDS agent, lease_expires_at;

DEFINE INDEX agent_task_retention_pin ON agent_task FIELDS task_id, retention_pin_active;

DEFINE INDEX agent_task_unique ON agent_task FIELDS agent, task_id UNIQUE;

DEFINE FIELD OVERWRITE active_generation ON managed_agent TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE active_revision ON managed_agent TYPE none | record<agent_definition_revision> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE admission_count ON managed_agent TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON managed_agent TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE definition ON managed_agent TYPE record<agent_definition> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE deployed_by ON managed_agent TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE desired ON managed_agent TYPE 'running' | 'paused' | 'archived' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dispatch_epoch ON managed_agent TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE generation ON managed_agent TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON managed_agent TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE identity ON managed_agent TYPE object ASSERT IF type::is_object($value) THEN object::keys($value) ALLINSIDE ['client_id', 'issuer', 'authorization_server', 'profile', 'resource', 'scopes', 'roles', 'membership'] AND $value.client_id != NONE AND $value.issuer != NONE AND $value.authorization_server != NONE AND $value.profile != NONE AND $value.resource != NONE AND $value.scopes != NONE AND $value.roles != NONE AND $value.membership != NONE ELSE false END;
DEFINE FIELD OVERWRITE identity.client_id ON managed_agent TYPE string;
DEFINE FIELD OVERWRITE identity.issuer ON managed_agent TYPE string;
DEFINE FIELD OVERWRITE identity.authorization_server ON managed_agent TYPE string;
DEFINE FIELD OVERWRITE identity.profile ON managed_agent TYPE string;
DEFINE FIELD OVERWRITE identity.resource ON managed_agent TYPE string;
DEFINE FIELD OVERWRITE identity.scopes ON managed_agent TYPE array<string>;
DEFINE FIELD OVERWRITE identity.roles ON managed_agent TYPE array<string>;
DEFINE FIELD OVERWRITE identity.membership ON managed_agent TYPE 'viewer' | 'contributor' | 'custodian' | 'owner';

DEFINE FIELD OVERWRITE key ON managed_agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON managed_agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE observed ON managed_agent TYPE 'queued' | 'credentials' | 'storage' | 'draining' | 'workload' | 'ready' | 'paused' | 'archived' | 'failed' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE operation ON managed_agent TYPE record<managed_agent_operation> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON managed_agent TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE principal ON managed_agent TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE public_key ON managed_agent TYPE option<object> ASSERT IF type::is_object($value) THEN object::keys($value) ALLINSIDE ['kid', 'n', 'e'] AND $value.kid != NONE AND $value.n != NONE AND $value.e != NONE ELSE $value = NONE END;
DEFINE FIELD OVERWRITE public_key.kid ON managed_agent TYPE option<string>;
DEFINE FIELD OVERWRITE public_key.n ON managed_agent TYPE option<string>;
DEFINE FIELD OVERWRITE public_key.e ON managed_agent TYPE option<string>;

DEFINE FIELD OVERWRITE requested_revision ON managed_agent TYPE record<agent_definition_revision> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE resources ON managed_agent TYPE object ASSERT IF type::is_object($value) THEN object::keys($value) ALLINSIDE ['namespace', 'workload', 'credential_secret', 'volume_claim', 'template_config_map', 'image', 'storage_gib'] AND $value.namespace != NONE AND $value.workload != NONE AND $value.credential_secret != NONE AND $value.volume_claim != NONE AND $value.template_config_map != NONE AND $value.image != NONE AND $value.storage_gib != NONE ELSE false END;
DEFINE FIELD OVERWRITE resources.namespace ON managed_agent TYPE string;
DEFINE FIELD OVERWRITE resources.workload ON managed_agent TYPE string;
DEFINE FIELD OVERWRITE resources.credential_secret ON managed_agent TYPE string;
DEFINE FIELD OVERWRITE resources.volume_claim ON managed_agent TYPE string;
DEFINE FIELD OVERWRITE resources.template_config_map ON managed_agent TYPE string;
DEFINE FIELD OVERWRITE resources.image ON managed_agent TYPE string;
DEFINE FIELD OVERWRITE resources.storage_gib ON managed_agent TYPE int ASSERT $value >= 0 AND $value <= 4294967295;

DEFINE FIELD OVERWRITE tenant ON managed_agent TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON managed_agent TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON managed_agent TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX managed_agent_client ON managed_agent FIELDS identity.client_id UNIQUE;

DEFINE INDEX managed_agent_credential ON managed_agent FIELDS resources.namespace, resources.credential_secret UNIQUE;

DEFINE INDEX managed_agent_key ON managed_agent FIELDS tenant, key UNIQUE;

DEFINE INDEX managed_agent_scope ON managed_agent FIELDS tenant, work_context, key;

DEFINE INDEX managed_agent_volume ON managed_agent FIELDS resources.namespace, resources.volume_claim UNIQUE;

DEFINE INDEX managed_agent_workload ON managed_agent FIELDS resources.namespace, resources.workload UNIQUE;

DEFINE FIELD OVERWRITE id ON managed_agent_capacity TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE instances ON managed_agent_capacity TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE storage_gib ON managed_agent_capacity TYPE int ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON managed_agent_capacity TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON managed_agent_capacity TYPE record<work_context> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE actor ON managed_agent_operation TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON managed_agent_operation TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE fingerprint ON managed_agent_operation TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE generation ON managed_agent_operation TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON managed_agent_operation TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE instance ON managed_agent_operation TYPE record<managed_agent> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_expires_at ON managed_agent_operation TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_fence ON managed_agent_operation TYPE int DEFAULT 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_owner ON managed_agent_operation TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE message ON managed_agent_operation TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE phase ON managed_agent_operation TYPE 'queued' | 'credentials' | 'storage' | 'draining' | 'workload' | 'ready' | 'paused' | 'archived' | 'failed' | 'superseded' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE request_id ON managed_agent_operation TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON managed_agent_operation TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON managed_agent_operation TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON managed_agent_operation TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX managed_operation_queue ON managed_agent_operation FIELDS phase, lease_expires_at;

DEFINE FIELD OVERWRITE acked_at ON wake TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE acked_by_episode ON wake TYPE none | record<agent_episode> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE agent ON wake TYPE record<agent> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE attempts ON wake TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE available_at ON wake TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE claim_expires_at ON wake TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE claim_fence ON wake TYPE none | int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE claimed_at ON wake TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE claimed_by ON wake TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE coalesced_into ON wake TYPE none | record<wake> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON wake TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dedupe_key ON wake TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON wake TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE kind ON wake TYPE 'task_result' | 'resource_changed' | 'timer' | 'operator_message' | 'input_request' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE last_error ON wake TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE payload ON wake TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON wake TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON wake TYPE 'pending' | 'claimed' | 'acked' | 'coalesced' | 'failed' DEFAULT 'pending' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON wake TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON wake TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX wake_agent_available ON wake FIELDS agent, state, available_at;

DEFINE INDEX wake_claim_expires ON wake FIELDS agent, state, claim_expires_at;

DEFINE INDEX wake_claim_expiry ON wake FIELDS state, claimed_at;

DEFINE INDEX wake_pending_count ON wake COUNT WHERE state = 'pending';

DEFINE INDEX wake_tenant_dedupe ON wake FIELDS tenant, agent, dedupe_key;
