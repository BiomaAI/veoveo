SELECT *
FROM ONLY $grant
WHERE
  computer_id = $computer_id
  AND owner_key = $owner_key
  AND provider_instance_id = $provider
  AND authority.invocation.tenant = $owner_tenant
  AND authority.invocation.work_context = $owner_context
  AND authority.actor.id = $owner_principal
  AND authority.actor.kind = $owner_kind
  AND authority.actor.issuer = $owner_issuer
  AND authority.actor.subject = $owner_subject;
