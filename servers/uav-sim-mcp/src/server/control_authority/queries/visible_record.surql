SELECT * FROM ONLY $record WHERE tenant = $tenant AND work_context = $work_context AND ($include_all OR principal_key = $principal);
