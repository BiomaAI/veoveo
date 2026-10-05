SELECT id, key, tenant FROM agent_definition WHERE id IN $definitions LIMIT 200;
SELECT * FROM agent_definition_revision WHERE id IN $revisions AND definition IN $definitions LIMIT 400;
