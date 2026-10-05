SELECT * FROM ONLY $record WHERE tenant = $tenant AND work_context = $context AND $labels CONTAINSALL authority.data_labels;
