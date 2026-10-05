SELECT * FROM time_authority_release
WHERE tenant = $tenant
ORDER BY created_at DESC;
