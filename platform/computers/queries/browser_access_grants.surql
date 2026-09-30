SELECT *
FROM computer_session_grant
WHERE
  computer_id = $computer_id
  AND owner_key = $owner_key
  AND provider_instance_id = $provider
  AND authority.invocation.tenant = $owner_tenant
  AND authority.invocation.work_context = $owner_context
  AND authority.actor.id = $owner_principal
  AND authority.actor.kind = $owner_kind
  AND authority.actor.issuer = $owner_issuer
  AND authority.actor.subject = $owner_subject
  AND revoked_at = NONE
  AND expires_at > time::now()
  AND idle_expires_at > time::now()
  AND family.revoked_at = NONE
  AND family.expires_at > time::now()
  AND (
    connection_id != NONE
    OR ticket_expires_at > time::now()
  )
ORDER BY grant_id DESC
LIMIT 129;
