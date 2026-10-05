SELECT * FROM time_source
WHERE tenant = $tenant
ORDER BY name ASC;
