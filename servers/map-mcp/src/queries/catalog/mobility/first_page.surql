SELECT * FROM map_mobility_profile WHERE tenant = $tenant
             ORDER BY profile_key ASC, profile_version ASC LIMIT $limit TIMEOUT 5s;
