UPDATE domain_usage SET tenant = tenant:wrong WHERE task = $task RETURN NONE;
