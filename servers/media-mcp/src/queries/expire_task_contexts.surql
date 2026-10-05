DELETE media_task_context
WHERE capability_expires_at <= time::now() RETURN BEFORE;
