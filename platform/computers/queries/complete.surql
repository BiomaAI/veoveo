SELECT * FROM computer
WHERE tenant_key = $owner_tenant
  AND owner_context.principal_key = $owner_principal
  AND owner_context.authority.work_context = $owner_context
  AND string::starts_with(<string>computer_id, $prefix)
ORDER BY computer_id ASC
LIMIT 101;
