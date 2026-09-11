SELECT VALUE grant_id FROM computer_automation_grant
WHERE grantee = $grantee AND computer_id = $computer AND provider_instance_id = $provider
  AND oauth_client_id = $client AND authority.profile = $profile
  AND authority.invocation.work_context = $context
  AND revoked_at = NONE AND expires_at > time::now()
ORDER BY grant_id LIMIT 64;
