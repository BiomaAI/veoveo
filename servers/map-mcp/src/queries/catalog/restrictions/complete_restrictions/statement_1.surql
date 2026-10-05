SELECT VALUE restriction_key FROM map_restriction WHERE tenant = $tenant
             AND string::lowercase(restriction_key) CONTAINS $needle
             GROUP BY restriction_key ORDER BY restriction_key ASC LIMIT 101 TIMEOUT 5s;
