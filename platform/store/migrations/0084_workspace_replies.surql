DEFINE FIELD OVERWRITE reply_to ON workspace_message TYPE option<record<workspace_message | workspace_run>>;
DEFINE FIELD reply_context ON workspace_message TYPE option<object>;
DEFINE FIELD reply_context.author_name ON workspace_message TYPE option<string>;
DEFINE FIELD reply_context.text ON workspace_message TYPE option<string>;
