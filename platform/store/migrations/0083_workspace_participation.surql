-- Absence on existing chats means explicit requests. Historical text is immutable.
DEFINE FIELD participation ON workspace_chat TYPE option<object>;
DEFINE FIELD participation.mode ON workspace_chat TYPE 'on_request' | 'default' | 'automatic';
DEFINE FIELD participation.agents ON workspace_chat TYPE array<record<workspace_agent>>;
DEFINE FIELD addressed_agents ON workspace_message TYPE option<array<record<workspace_agent>>>;
DEFINE FIELD response_agents ON workspace_message TYPE option<array<record<workspace_agent>>>;
DEFINE FIELD OVERWRITE failure ON workspace_run TYPE option<'model_unavailable' | 'permission_changed' | 'output_limit' | 'deadline' | 'worker_lost' | 'capacity'>;
