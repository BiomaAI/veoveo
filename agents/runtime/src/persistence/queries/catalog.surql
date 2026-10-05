RETURN SELECT
  { key: key, name: name, description: description, revision: revision,
    digest: published.digest, model: published.model, tools: published.tools,
    execution_kind: published.execution.kind } AS entry,
  published.* AS revision
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
