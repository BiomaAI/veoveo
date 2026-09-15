-- Additive Workspace agent/run state. Existing human history remains immutable.
DEFINE TABLE workspace_agent SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON workspace_agent TYPE uuid;
DEFINE FIELD chat ON workspace_agent TYPE record<workspace_chat>;
DEFINE FIELD definition ON workspace_agent TYPE string;
DEFINE FIELD definition_digest ON workspace_agent TYPE string;
DEFINE FIELD display_name ON workspace_agent TYPE string;
DEFINE FIELD provider ON workspace_agent TYPE string;
DEFINE FIELD model ON workspace_agent TYPE string;
DEFINE FIELD active ON workspace_agent TYPE bool DEFAULT true;
DEFINE FIELD joined_at ON workspace_agent TYPE datetime DEFAULT time::now();
DEFINE INDEX workspace_agent_identity ON workspace_agent FIELDS chat, definition UNIQUE;

DEFINE TABLE workspace_run SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON workspace_run TYPE uuid;
DEFINE FIELD chat ON workspace_run TYPE record<workspace_chat>;
DEFINE FIELD agent ON workspace_run TYPE record<workspace_agent>;
DEFINE FIELD definition_digest ON workspace_run TYPE string;
DEFINE FIELD initiator ON workspace_run TYPE record<principal>;
DEFINE FIELD trigger ON workspace_run TYPE record<workspace_message>;
DEFINE FIELD context_sequence ON workspace_run TYPE int;
DEFINE FIELD sequence ON workspace_run TYPE int;
DEFINE FIELD updated_sequence ON workspace_run TYPE int;
DEFINE FIELD state ON workspace_run TYPE 'queued' | 'running' | 'completed' | 'cancelled' | 'interrupted' | 'failed';
DEFINE FIELD text ON workspace_run TYPE string DEFAULT '' ASSERT string::len($value) <= 32768;
DEFINE FIELD failure ON workspace_run TYPE option<'model_unavailable' | 'permission_changed' | 'output_limit' | 'deadline' | 'worker_lost'>;
DEFINE FIELD fence ON workspace_run TYPE option<uuid>;
DEFINE FIELD lease_until ON workspace_run TYPE datetime;
DEFINE FIELD deadline ON workspace_run TYPE datetime;
DEFINE FIELD created_at ON workspace_run TYPE datetime DEFAULT time::now();
DEFINE FIELD updated_at ON workspace_run TYPE datetime DEFAULT time::now();
DEFINE INDEX workspace_run_trigger ON workspace_run FIELDS trigger, agent UNIQUE;
DEFINE INDEX workspace_run_order ON workspace_run FIELDS chat, sequence UNIQUE;
DEFINE INDEX workspace_run_changes ON workspace_run FIELDS chat, updated_sequence;
DEFINE INDEX workspace_run_recovery ON workspace_run FIELDS state, lease_until;

DEFINE FIELD OVERWRITE kind ON workspace_event TYPE 'created' | 'message' | 'invitation' | 'membership' | 'settings' | 'agent' | 'run';
DEFINE FIELD OVERWRITE target ON workspace_event TYPE record<workspace_chat | workspace_message | workspace_invitation | workspace_member | workspace_agent | workspace_run>;

-- Keep the original creator independent of mutable ownership for request replay.
DEFINE FIELD created_by ON workspace_chat TYPE option<record<principal>>;
UPDATE workspace_chat SET created_by = owner WHERE created_by = NONE;
