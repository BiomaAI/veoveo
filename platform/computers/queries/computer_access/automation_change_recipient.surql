SELECT VALUE grant_id FROM $grant WHERE grantee = $grantee
             AND principal_id = $grantee_principal AND grantee_kind = $grantee_kind
             AND grantee_issuer = $grantee_issuer AND grantee_subject = $grantee_subject
             AND authority.invocation.tenant = $grantee_tenant
             AND computer_id = $computer AND provider_instance_id = $provider
             AND oauth_client_id = $client AND authority.profile = $profile
             AND authority.invocation.work_context = $context AND 'read' IN permissions;
