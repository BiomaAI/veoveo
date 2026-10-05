RETURN SELECT * FROM workspace_agent WHERE chat.tenant = $authority.tenant AND chat.work_context = $authority.work_context AND definition IN $command LIMIT 10001;
