SELECT *
FROM ONLY $grant
WHERE
  provider_instance_id = $provider
  AND ticket_hash = $ticket_hash
  AND fn::kernel::gateway::refresh_family_current_v1(family, time::now()) != NONE
  AND connection_id = NONE
  AND revoked_at = NONE
  AND ticket_expires_at > time::now()
  AND authority.invocation.tenant = $owner_tenant
  AND authority.invocation.work_context = $context
  AND authority.actor.id = $owner_principal
  AND authority.actor.kind = $owner_kind
  AND authority.actor.issuer = $owner_issuer
  AND authority.actor.subject = $owner_subject
  AND authority.request_context.principal.id = $grantee_principal
  AND authority.request_context.principal.kind = $grantee_kind
  AND authority.request_context.principal.issuer = $grantee_issuer
  AND authority.request_context.principal.subject = $grantee_subject
  AND authority.profile = $profile
  AND authority.request_context.access_token.oauth_client_id = $client
  AND authority.request_context.access_token.session_family = $session_family;
