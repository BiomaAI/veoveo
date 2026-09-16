SELECT *
FROM computer
WHERE
  tenant_key = $owner_tenant
  AND owner_context.principal_key = $owner_principal
  AND owner_context.authority.work_context = $owner_context
  AND (
    $after = NONE
    OR computer_id > $after
  )
ORDER BY computer_id ASC
LIMIT $limit;
