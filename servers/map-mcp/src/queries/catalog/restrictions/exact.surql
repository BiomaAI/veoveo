SELECT * FROM map_restriction WHERE tenant = $tenant AND restriction_key = $key
             ORDER BY restriction_key ASC LIMIT $limit TIMEOUT 5s;
