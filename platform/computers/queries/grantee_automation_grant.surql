SELECT *
FROM ONLY $grant
WHERE
  computer_id = $computer
  AND provider_instance_id = $provider
  AND grantee = $grantee
  AND principal_id = $grantee_principal
  AND grantee_kind = $grantee_kind
  AND grantee_issuer = $grantee_issuer
  AND grantee_subject = $grantee_subject
  AND oauth_client_id = $client
  AND authority.invocation.tenant = $grantee_tenant
  AND authority.invocation.work_context = $context
  AND authority.profile = $profile;
