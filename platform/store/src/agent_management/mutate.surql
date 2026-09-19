IF $authority.membership = 'viewer' { THROW 'agent_forbidden'; };
LET $definition = SELECT * FROM ONLY $command.definition;
IF $definition != NONE AND !fn::agent_definition_editor($authority, $definition) { THROW 'agent_not_found'; };
LET $receipt = SELECT * FROM ONLY $command.receipt;
IF $receipt != NONE {
    IF $receipt.fingerprint != $command.fingerprint OR $receipt.work_context != $authority.work_context { THROW 'agent_conflict'; };
    RETURN $receipt.result;
};
LET $mutation = $command.mutation;
IF $mutation.kind = 'create' {
    IF $definition != NONE { THROW 'agent_conflict'; };
    LET $capacity = type::record('agent_catalog_capacity', $authority.work_context.id);
    UPSERT ONLY $capacity SET tenant = $authority.tenant, work_context = $authority.work_context, definitions = (definitions ?? 0) + 1;
    IF $capacity.definitions > $authority.definition_limit { THROW 'agent_capacity'; };
    CREATE ONLY $command.definition SET tenant = $authority.tenant, work_context = $authority.work_context,
        owner = $authority.principal, key = $command.key, name = $mutation.name, description = $mutation.description,
        revision = 1, status = 'enabled', disabled = false, audience = [], draft = $mutation.content, draft_digest = $command.content_digest;
} ELSE {
    IF $definition = NONE { THROW 'agent_not_found'; };
    IF $definition.revision != $command.expected_revision { THROW 'agent_conflict'; };
    IF $definition.status = 'archived' AND $mutation.kind NOT IN ['transfer', 'status'] { THROW 'agent_conflict'; };
    IF $mutation.kind = 'draft' {
        UPDATE ONLY $command.definition SET draft = $mutation.content, draft_digest = $command.content_digest;
    } ELSE IF $mutation.kind = 'metadata' {
        UPDATE ONLY $command.definition SET name = $mutation.name, description = $mutation.description;
    } ELSE IF $mutation.kind = 'publish' {
        IF $definition.disabled OR $definition.draft_digest != $mutation.digest { THROW 'agent_conflict'; };
        FOR $target IN $mutation.audience {
            LET $context = SELECT * FROM ONLY $target.work_context;
            IF $context = NONE OR $context.tenant != $authority.tenant OR
                crypto::sha256(<string> [$context.policy_revision, $context.memberships, $context.output_policy]) != $target.context_digest { THROW 'agent_forbidden'; };
        };
        LET $existing = array::first(SELECT id FROM agent_definition_revision WHERE definition = $command.definition AND digest = $mutation.digest LIMIT 1);
        LET $published = IF $existing = NONE {
            LET $created = CREATE ONLY agent_definition_revision SET definition = $command.definition, digest = $mutation.digest,
                content = $definition.draft, created_by = $authority.principal;
            RETURN $created.id;
        } ELSE { RETURN $existing.id; };
        UPDATE ONLY $command.definition SET published = $published, audience = $mutation.audience.map(|$target| $target.work_context);
    } ELSE IF $mutation.kind = 'status' {
        IF $definition.status = 'archived' AND $mutation.status = 'enabled' { THROW 'agent_conflict'; };
        UPDATE ONLY $command.definition SET
            status = IF $definition.status = 'archived' { 'archived' } ELSE { $mutation.status },
            disabled = IF $mutation.status = 'archived' { $definition.disabled } ELSE { $mutation.status = 'disabled' };
    } ELSE IF $mutation.kind = 'transfer' {
        LET $owner = SELECT * FROM ONLY $mutation.owner;
        IF $owner = NONE OR !$owner.enabled OR $owner.tenant != $authority.tenant { THROW 'agent_forbidden'; };
        UPDATE ONLY $command.definition SET owner = $mutation.owner;
    } ELSE { THROW 'agent_conflict'; };
    UPDATE ONLY $command.definition SET revision += 1, updated_at = time::now();
};
LET $result = SELECT * FROM ONLY $command.definition;
CREATE ONLY $command.receipt SET tenant = $authority.tenant, work_context = $authority.work_context,
    actor = $authority.principal, request_id = $command.request_id, fingerprint = $command.fingerprint, result = $result;
CREATE outbox_event SET tenant = $authority.tenant, aggregate_type = 'agent_definition', aggregate_id = $command.key,
    event_type = string::concat('agent_definition.', $mutation.kind), schema_version = 1,
    payload = { key: $command.key, revision: $result.revision, work_context: $authority.work_context,
        affected_contexts: array::distinct(array::concat([$authority.work_context], $result.audience, $definition.audience ?? [])),
        actor: $authority.principal, request_id: $command.request_id };
RETURN $result;
