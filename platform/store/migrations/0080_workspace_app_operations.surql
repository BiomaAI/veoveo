-- App-origin binding belongs to the existing durable operation journal.
-- Existing chat/model operations remain unbound and cannot be claimed by an App.
DEFINE FIELD app_uri ON workspace_operation TYPE option<string>
    ASSERT $value = NONE OR string::len($value) <= 2048;
DEFINE INDEX workspace_operation_app_task ON workspace_operation
    FIELDS tenant, work_context, owner, profile, app_uri, task_id;
