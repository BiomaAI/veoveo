SELECT * FROM ONLY $upload
WHERE tenant = $tenant AND tenant_key = $tenant_key
    AND actor = $actor AND actor_key = $actor_key
    AND profile_key = $profile
    AND work_context = $context AND authority.context_key = $context_key
    AND actor_issuer = $issuer AND actor_subject = $subject;
