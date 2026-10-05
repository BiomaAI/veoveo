SELECT VALUE <string> id FROM workspace_run WHERE chat.tenant = $tenant AND chat.work_context = $context AND state IN ['queued', 'running'] LIMIT 1;
