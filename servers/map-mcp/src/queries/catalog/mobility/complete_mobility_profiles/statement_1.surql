SELECT VALUE profile_key FROM map_mobility_profile WHERE tenant = $tenant
             AND string::lowercase(profile_key) CONTAINS $needle
             GROUP BY profile_key ORDER BY profile_key ASC LIMIT 101 TIMEOUT 5s;
