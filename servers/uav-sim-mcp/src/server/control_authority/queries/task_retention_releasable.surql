SELECT * FROM ONLY $plan WHERE tenant = $tenant AND work_context = $context AND principal_key = $principal;
             SELECT VALUE (plan = $plan AND task = $task AND tenant = $tenant AND work_context = $context AND principal_key = $principal) FROM ONLY $execution;

SELECT * FROM ONLY $catalog;
