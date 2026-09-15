-- Private operation receipts survive browser, BFF and gateway restarts.
-- Native MCP owns Task state; this table only retains dispatch identity and routing.
DEFINE TABLE workspace_operation SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON workspace_operation TYPE uuid;
DEFINE FIELD tenant ON workspace_operation TYPE record<tenant>;
DEFINE FIELD work_context ON workspace_operation TYPE record<work_context>;
DEFINE FIELD owner ON workspace_operation TYPE record<principal>;
DEFINE FIELD chat ON workspace_operation TYPE record<workspace_chat>;
DEFINE FIELD run ON workspace_operation TYPE option<record<workspace_run>>;
DEFINE FIELD profile ON workspace_operation TYPE string;
DEFINE FIELD tool ON workspace_operation TYPE string;
DEFINE FIELD arguments ON workspace_operation TYPE string ASSERT string::len($value) <= 65536;
DEFINE FIELD phase ON workspace_operation TYPE 'dispatching' | 'input_required' | 'task' | 'completed' | 'failed' | 'unconfirmed';
DEFINE FIELD fence ON workspace_operation TYPE uuid;
DEFINE FIELD revision ON workspace_operation TYPE int DEFAULT 1;
DEFINE FIELD round ON workspace_operation TYPE int DEFAULT 0;
DEFINE FIELD dispatch_until ON workspace_operation TYPE datetime;
DEFINE FIELD task_id ON workspace_operation TYPE option<string>;
DEFINE FIELD response ON workspace_operation TYPE option<string> ASSERT $value = NONE OR string::len($value) <= 1048576;
DEFINE FIELD created_at ON workspace_operation TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON workspace_operation TYPE datetime DEFAULT time::now();
DEFINE INDEX workspace_operation_owner ON workspace_operation FIELDS tenant, work_context, owner, created_at;
DEFINE INDEX workspace_operation_chat ON workspace_operation FIELDS tenant, work_context, owner, chat, created_at;

DEFINE FUNCTION fn::workspace_operation_owner($authority: object, $id: record<workspace_operation>) {
    LET $operation = SELECT * FROM ONLY $id;
    RETURN $operation != NONE AND $operation.tenant = $authority.tenant
        AND $operation.work_context = $authority.work_context AND $operation.owner = $authority.principal;
} PERMISSIONS NONE;
