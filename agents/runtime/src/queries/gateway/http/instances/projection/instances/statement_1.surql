SELECT id, key FROM agent_definition WHERE id IN $definitions LIMIT 200; SELECT id, digest, content.execution.template AS template FROM agent_definition_revision WHERE id IN $revisions LIMIT 400;
