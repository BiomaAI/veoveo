CREATE ONLY $context SET tenant = $tenant, context_key = 'shared', title = 'Shared work',
        policy_revision = 'v1', memberships = $rules,
        output_policy = {owner_kind: 'principal', owner_key: 'Alice', initial_grants: [], data_labels: []};