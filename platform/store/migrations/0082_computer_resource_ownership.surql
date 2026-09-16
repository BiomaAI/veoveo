-- Cross-client resource discovery uses retained identity metadata. Historical
-- owner keys remain unchanged because private envelopes and journals bind them.
DEFINE INDEX computer_resource_owner ON computer
    FIELDS tenant_key, owner_context.principal_key, owner_context.authority.work_context, computer_id;
