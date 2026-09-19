RETURN SELECT
  key,
  name,
  description,
  revision,
  published.digest AS digest,
  published.content.model AS model,
  published.content.tools AS tools,
  published.content.execution.kind AS execution_kind
FROM agent_definition
WHERE
  tenant = $authority.tenant
  AND status = 'enabled'
  AND disabled = false
  AND $authority.work_context IN audience
  AND published != NONE
  AND key > $command.after
ORDER BY key
LIMIT $command.limit;
