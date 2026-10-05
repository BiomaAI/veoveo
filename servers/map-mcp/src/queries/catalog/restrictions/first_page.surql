SELECT * FROM map_restriction WHERE tenant = $tenant
             ORDER BY restriction_key ASC LIMIT $limit TIMEOUT 5s;
