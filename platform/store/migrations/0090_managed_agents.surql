-- Managed lifecycle records. The service registration holds public keys only.
DEFINE TABLE managed_agent SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON managed_agent TYPE uuid;
DEFINE FIELD tenant ON managed_agent TYPE record<tenant>;
DEFINE FIELD work_context ON managed_agent TYPE record<work_context>;
DEFINE FIELD owner ON managed_agent TYPE record<principal>;
DEFINE FIELD deployed_by ON managed_agent TYPE record<principal>;
DEFINE FIELD key ON managed_agent TYPE string;
DEFINE FIELD name ON managed_agent TYPE string;
DEFINE FIELD definition ON managed_agent TYPE record<agent_definition>;
DEFINE FIELD requested_revision ON managed_agent TYPE record<agent_definition_revision>;
DEFINE FIELD active_revision ON managed_agent TYPE option<record<agent_definition_revision>>;
DEFINE FIELD generation ON managed_agent TYPE int ASSERT $value > 0;
DEFINE FIELD active_generation ON managed_agent TYPE int ASSERT $value >= 0;
DEFINE FIELD dispatch_epoch ON managed_agent TYPE int ASSERT $value > 0;
DEFINE FIELD desired ON managed_agent TYPE 'running' | 'paused' | 'archived';
DEFINE FIELD observed ON managed_agent TYPE 'queued' | 'credentials' | 'storage' | 'draining' | 'workload' | 'ready' | 'paused' | 'archived' | 'failed';
DEFINE FIELD principal ON managed_agent TYPE record<principal>;
DEFINE FIELD identity ON managed_agent TYPE object FLEXIBLE;
DEFINE FIELD resources ON managed_agent TYPE object FLEXIBLE;
DEFINE FIELD public_key ON managed_agent TYPE option<object> FLEXIBLE;
DEFINE FIELD operation ON managed_agent TYPE record<managed_agent_operation>;
DEFINE FIELD created_at ON managed_agent TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON managed_agent TYPE datetime DEFAULT time::now();
DEFINE INDEX managed_agent_key ON managed_agent FIELDS tenant, key UNIQUE;
DEFINE INDEX managed_agent_client ON managed_agent FIELDS identity.client_id UNIQUE;
DEFINE INDEX managed_agent_workload ON managed_agent FIELDS resources.namespace, resources.workload UNIQUE;
DEFINE INDEX managed_agent_credential ON managed_agent FIELDS resources.namespace, resources.credential_secret UNIQUE;
DEFINE INDEX managed_agent_volume ON managed_agent FIELDS resources.namespace, resources.volume_claim UNIQUE;
DEFINE INDEX managed_agent_scope ON managed_agent FIELDS tenant, work_context, key;

DEFINE TABLE managed_agent_operation SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON managed_agent_operation TYPE uuid;
DEFINE FIELD instance ON managed_agent_operation TYPE record<managed_agent>;
DEFINE FIELD tenant ON managed_agent_operation TYPE record<tenant>;
DEFINE FIELD work_context ON managed_agent_operation TYPE record<work_context>;
DEFINE FIELD actor ON managed_agent_operation TYPE record<principal>;
DEFINE FIELD request_id ON managed_agent_operation TYPE uuid;
DEFINE FIELD fingerprint ON managed_agent_operation TYPE string;
DEFINE FIELD generation ON managed_agent_operation TYPE int ASSERT $value > 0;
DEFINE FIELD phase ON managed_agent_operation TYPE 'queued' | 'credentials' | 'storage' | 'draining' | 'workload' | 'ready' | 'paused' | 'archived' | 'failed' | 'superseded';
DEFINE FIELD message ON managed_agent_operation TYPE option<string>;
DEFINE FIELD lease_owner ON managed_agent_operation TYPE option<uuid>;
DEFINE FIELD lease_fence ON managed_agent_operation TYPE int DEFAULT 0;
DEFINE FIELD lease_expires_at ON managed_agent_operation TYPE option<datetime>;
DEFINE FIELD created_at ON managed_agent_operation TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON managed_agent_operation TYPE datetime DEFAULT time::now();
DEFINE INDEX managed_operation_queue ON managed_agent_operation FIELDS phase, lease_expires_at;

DEFINE TABLE managed_agent_capacity SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON managed_agent_capacity TYPE uuid;
DEFINE FIELD tenant ON managed_agent_capacity TYPE record<tenant>;
DEFINE FIELD work_context ON managed_agent_capacity TYPE record<work_context>;
DEFINE FIELD instances ON managed_agent_capacity TYPE int ASSERT $value >= 0;
DEFINE FIELD storage_gib ON managed_agent_capacity TYPE int ASSERT $value >= 0;

DEFINE FUNCTION fn::managed_agent_editor($authority: object, $instance: any) -> bool {
    RETURN $instance != NONE AND $instance.tenant = $authority.tenant
        AND $instance.work_context = $authority.work_context
        AND ($instance.owner = $authority.principal OR $authority.manage_context);
};

DEFINE FUNCTION fn::managed_agent_enabled($id: record<managed_agent>) -> bool {
    LET $instance = SELECT * FROM ONLY $id;
    RETURN $instance != NONE AND $instance.desired != 'archived'
        AND $instance.tenant.enabled = true AND $instance.principal.enabled = true
        AND $instance.work_context.tenant = $instance.tenant
        AND $instance.definition.tenant = $instance.tenant AND $instance.definition.disabled = false
        AND $instance.work_context IN $instance.definition.audience;
};

DEFINE FUNCTION fn::managed_agent_claim($claim: object) -> bool {
    LET $operation = SELECT * FROM ONLY $claim.operation;
    RETURN $operation != NONE AND $operation.instance = $claim.instance
        AND $operation.generation = $claim.generation AND $operation.lease_owner = $claim.owner
        AND $operation.lease_fence = $claim.fence AND $operation.lease_expires_at > time::now()
        AND $operation.instance.operation = $claim.operation AND $operation.instance.generation = $claim.generation;
};
