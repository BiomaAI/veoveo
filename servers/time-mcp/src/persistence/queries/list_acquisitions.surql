SELECT * FROM time_acquisition
WHERE tenant = $tenant
ORDER BY created_at DESC;
