LET $key = SELECT computer_id,
binding.owner_key AS owner_key
FROM ONLY $execution
WHERE provider_instance_id = $provider;
LET $owned = SELECT VALUE computer_id
FROM computer
WHERE
  tenant_key = $owner_tenant
  AND owner_context.authority.tenant = $owner_tenant
  AND (
    owner_context.tenant_key ?? 'installation'
  ) = $owner_tenant
  AND owner_context.principal_key = $owner_principal
  AND owner_context.principal_kind = $owner_kind
  AND owner_context.issuer = $owner_issuer
  AND owner_context.subject = $owner_subject
  AND owner_context.authority.work_context = $owner_context
  AND owner_context.data_labels ALLINSIDE $owner_labels
  AND (
    owner_context.authority.output_policy.data_labels ?? []
  ) ALLINSIDE $owner_labels
  AND (
    owner_context.authority.output_policy.classification = NONE
    OR owner_context.authority.output_policy.classification IN $owner_labels
  )
  AND $direct_owner = true
  AND provider_instance_id = $provider
  AND computer_id = $key.computer_id
  AND owner_key = $key.owner_key;
SELECT
  computer_id,
  binding.owner_key AS owner_key,
  (
    binding.grant_id ?? NONE
  ) AS grant_id,
  computer_id IN $owned AS owned
FROM ONLY $execution
WHERE
  provider_instance_id = $provider
  AND authority.invocation.tenant = $owner_tenant
  AND (
    IF $file {
      binding.required_labels;
    } ELSE {
      binding.required_output_labels;
    }
  ) ALLINSIDE $owner_labels
  AND (
    IF $file {
      binding.required_labels;
    } ELSE {
      binding.required_output_labels;
    }
  ) ALLINSIDE $grantee_labels
  AND (
    (
      actor_key = $actor_key
      AND authority.request_context.principal.id = $grantee_principal
      AND authority.request_context.principal.kind = $grantee_kind
      AND authority.request_context.principal.issuer = $grantee_issuer
      AND authority.request_context.principal.subject = $grantee_subject
    )
    OR computer_id IN $owned
  );
