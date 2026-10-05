-- Current owner schema; schema definitions only.

DEFINE TABLE workspace_agent TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE workspace_agent_revision_receipt TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE workspace_chat TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE workspace_event TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE workspace_invitation TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE workspace_member TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE workspace_message TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE workspace_operation TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE workspace_run TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE FUNCTION fn::workspace_authority($authority: object) -> bool {
    LET $tenant: record<tenant> = $authority.tenant;
    LET $context: record<work_context> = $authority.work_context;
    LET $principal: record<principal> = $authority.principal;
    LET $digest: string = $authority.context_digest;
    RETURN fn::kernel::identity::actor_admitted_v1($tenant, $context, $principal, $digest, true);
} PERMISSIONS FULL;

DEFINE FUNCTION fn::workspace_member($authority: object, $chat: record<workspace_chat>) -> bool {
    IF !fn::workspace_authority($authority) { RETURN false; };
    LET $current = SELECT * FROM ONLY $chat;
    RETURN IF type::is_object($current) THEN
        $current.tenant = $authority.tenant AND $current.work_context = $authority.work_context AND
        count((SELECT id FROM workspace_member WHERE chat = $chat AND principal = $authority.principal AND active = true LIMIT 1)) = 1
    ELSE false END;
} PERMISSIONS FULL;

DEFINE FUNCTION fn::workspace_operation_owner($authority: object, $id: record<workspace_operation>) -> bool {
    LET $operation = SELECT * FROM ONLY $id;
    RETURN IF type::is_object($operation) THEN
        $operation.tenant = $authority.tenant AND $operation.work_context = $authority.work_context AND $operation.owner = $authority.principal
    ELSE false END;
} PERMISSIONS NONE;

DEFINE FIELD OVERWRITE active ON workspace_agent TYPE bool DEFAULT true PERMISSIONS FULL;

DEFINE FIELD OVERWRITE chat ON workspace_agent TYPE record<workspace_chat> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE definition ON workspace_agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE definition_digest ON workspace_agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE display_name ON workspace_agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON workspace_agent TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE joined_at ON workspace_agent TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE model ON workspace_agent TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider ON workspace_agent TYPE string PERMISSIONS FULL;

DEFINE INDEX workspace_agent_identity ON workspace_agent FIELDS chat, definition UNIQUE;

DEFINE FIELD OVERWRITE actor ON workspace_agent_revision_receipt TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE chat ON workspace_agent_revision_receipt TYPE record<workspace_chat> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON workspace_agent_revision_receipt TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE fingerprint ON workspace_agent_revision_receipt TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON workspace_agent_revision_receipt TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE result ON workspace_agent_revision_receipt TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE archived ON workspace_chat TYPE bool DEFAULT false PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON workspace_chat TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by ON workspace_chat TYPE none | record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON workspace_chat TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE initial_title ON workspace_chat TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE members_can_invite ON workspace_chat TYPE bool DEFAULT false PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON workspace_chat TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE participation ON workspace_chat TYPE none | object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE participation.agents ON workspace_chat TYPE array<record<workspace_agent>> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE participation.agents.* ON workspace_chat TYPE record<workspace_agent> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE participation.mode ON workspace_chat TYPE 'on_request' | 'default' | 'automatic' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON workspace_chat TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE `sequence` ON workspace_chat TYPE int DEFAULT 1 ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON workspace_chat TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE title ON workspace_chat TYPE string ASSERT string::len($value) INSIDE 1..=200 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON workspace_chat TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON workspace_chat TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX workspace_chat_context ON workspace_chat FIELDS tenant, work_context, updated_at;

DEFINE FIELD OVERWRITE chat ON workspace_event TYPE record<workspace_chat> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON workspace_event TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON workspace_event TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE kind ON workspace_event TYPE 'created' | 'message' | 'invitation' | 'membership' | 'settings' | 'agent' | 'run' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE `sequence` ON workspace_event TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE target ON workspace_event TYPE record<workspace_chat | workspace_message | workspace_invitation | workspace_member | workspace_agent | workspace_run> PERMISSIONS FULL;

DEFINE INDEX workspace_event_order ON workspace_event FIELDS chat, `sequence` UNIQUE;

DEFINE FIELD OVERWRITE chat ON workspace_invitation TYPE record<workspace_chat> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON workspace_invitation TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON workspace_invitation TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON workspace_invitation TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE invitee ON workspace_invitation TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE inviter ON workspace_invitation TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON workspace_invitation TYPE 'pending' | 'accepted' | 'declined' | 'revoked' DEFAULT 'pending' PERMISSIONS FULL;

DEFINE INDEX workspace_invitation_chat ON workspace_invitation FIELDS chat, state;

DEFINE INDEX workspace_invitation_inbox ON workspace_invitation FIELDS invitee, state, expires_at;

DEFINE FIELD OVERWRITE active ON workspace_member TYPE bool DEFAULT true PERMISSIONS FULL;

DEFINE FIELD OVERWRITE chat ON workspace_member TYPE record<workspace_chat> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON workspace_member TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE joined_at ON workspace_member TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE principal ON workspace_member TYPE record<principal> PERMISSIONS FULL;

DEFINE INDEX workspace_member_identity ON workspace_member FIELDS chat, principal UNIQUE;

DEFINE INDEX workspace_member_inbox ON workspace_member FIELDS principal, active, chat;

DEFINE FIELD OVERWRITE addressed_agents ON workspace_message TYPE none | array<record<workspace_agent>> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE addressed_agents.* ON workspace_message TYPE record<workspace_agent> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE attachments ON workspace_message TYPE none | array<object> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE attachments.* ON workspace_message TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE attachments.*.artifact ON workspace_message TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE attachments.*.name ON workspace_message TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE author ON workspace_message TYPE record<workspace_member> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE chat ON workspace_message TYPE record<workspace_chat> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON workspace_message TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON workspace_message TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE reply_context ON workspace_message TYPE none | object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE reply_context.author_name ON workspace_message TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE reply_context.text ON workspace_message TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE reply_to ON workspace_message TYPE none | record<workspace_message | workspace_run> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE response_agents ON workspace_message TYPE none | array<record<workspace_agent>> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE response_agents.* ON workspace_message TYPE record<workspace_agent> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE `sequence` ON workspace_message TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE text ON workspace_message TYPE string ASSERT string::len($value) <= 32768 PERMISSIONS FULL;

DEFINE INDEX workspace_message_order ON workspace_message FIELDS chat, `sequence` UNIQUE;

DEFINE FIELD OVERWRITE app_uri ON workspace_operation TYPE none | string ASSERT $value = NONE OR string::len($value) <= 2048 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE arguments ON workspace_operation TYPE string ASSERT string::len($value) <= 65536 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE chat ON workspace_operation TYPE record<workspace_chat> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON workspace_operation TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dispatch_until ON workspace_operation TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE fence ON workspace_operation TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON workspace_operation TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON workspace_operation TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE phase ON workspace_operation TYPE 'dispatching' | 'input_required' | 'task' | 'completed' | 'failed' | 'unconfirmed' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE profile ON workspace_operation TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE progress ON workspace_operation TYPE none | object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE progress.completed ON workspace_operation TYPE float ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE progress.message ON workspace_operation TYPE none | string ASSERT $value = NONE OR string::len($value) <= 2000 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE progress.total ON workspace_operation TYPE none | float ASSERT $value = NONE OR $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE response ON workspace_operation TYPE none | string ASSERT $value = NONE OR string::len($value) <= 1048576 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON workspace_operation TYPE int DEFAULT 1 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE round ON workspace_operation TYPE int DEFAULT 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE run ON workspace_operation TYPE none | record<workspace_run> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task_id ON workspace_operation TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON workspace_operation TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tool ON workspace_operation TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON workspace_operation TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON workspace_operation TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX workspace_operation_app_task ON workspace_operation FIELDS tenant, work_context, owner, profile, app_uri, task_id;

DEFINE INDEX workspace_operation_chat ON workspace_operation FIELDS tenant, work_context, owner, chat, created_at;

DEFINE INDEX workspace_operation_owner ON workspace_operation FIELDS tenant, work_context, owner, created_at;

DEFINE FIELD OVERWRITE agent ON workspace_run TYPE record<workspace_agent> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE chat ON workspace_run TYPE record<workspace_chat> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE context_sequence ON workspace_run TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON workspace_run TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE deadline ON workspace_run TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE definition_digest ON workspace_run TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE failure ON workspace_run TYPE none | 'model_unavailable' | 'permission_changed' | 'output_limit' | 'deadline' | 'worker_lost' | 'capacity' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE feedback ON workspace_run TYPE object DEFAULT { phase: 'preparing', operations: 0 } PERMISSIONS FULL;

DEFINE FIELD OVERWRITE feedback.operations ON workspace_run TYPE int ASSERT $value >= 0 AND $value <= 8 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE feedback.phase ON workspace_run TYPE 'preparing' | 'responding' | 'calling_tools' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE fence ON workspace_run TYPE none | uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON workspace_run TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE initiator ON workspace_run TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_until ON workspace_run TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE `sequence` ON workspace_run TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON workspace_run TYPE 'queued' | 'running' | 'completed' | 'cancelled' | 'interrupted' | 'failed' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE text ON workspace_run TYPE string DEFAULT '' ASSERT string::len($value) <= 32768 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE trigger ON workspace_run TYPE record<workspace_message> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON workspace_run TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_sequence ON workspace_run TYPE int PERMISSIONS FULL;

DEFINE INDEX workspace_run_changes ON workspace_run FIELDS chat, updated_sequence;

DEFINE INDEX workspace_run_order ON workspace_run FIELDS chat, `sequence` UNIQUE;

DEFINE INDEX workspace_run_recovery ON workspace_run FIELDS state, lease_until;

DEFINE INDEX workspace_run_trigger ON workspace_run FIELDS trigger, agent UNIQUE;
