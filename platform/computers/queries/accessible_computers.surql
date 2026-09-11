LET $owned = SELECT VALUE computer_id FROM computer
  WHERE owner_key = $owner AND provider_instance_id = $provider
    AND string::starts_with(<string>computer_id, $prefix)
    AND ($after = NONE OR computer_id > $after)
  ORDER BY computer_id LIMIT $limit;
LET $granted = SELECT VALUE computer_id FROM computer_automation_grant
  WHERE grantee = $grantee AND provider_instance_id = $provider
    AND oauth_client_id = $client AND authority.profile = $profile
    AND authority.invocation.work_context = $context AND "read" IN permissions
    AND string::starts_with(<string>computer_id, $prefix)
    AND revoked_at = NONE AND expires_at > time::now()
    AND ($after = NONE OR computer_id > $after)
  GROUP BY computer_id ORDER BY computer_id LIMIT $limit;
RETURN array::slice(array::sort(array::distinct(array::concat($owned, $granted))), 0, $limit);
