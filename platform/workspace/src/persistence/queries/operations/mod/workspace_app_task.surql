RETURN SELECT *, IF run = NONE { NONE } ELSE { { id: run.agent, name: run.agent.display_name } } AS agent FROM workspace_operation
            WHERE tenant = $authority.tenant AND work_context = $authority.work_context
            AND owner = $authority.principal AND profile = $command.profile
            AND app_uri = $command.app_uri AND task_id = $command.task_id LIMIT 1;
