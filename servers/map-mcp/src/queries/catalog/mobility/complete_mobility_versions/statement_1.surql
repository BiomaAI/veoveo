SELECT VALUE profile_version FROM map_mobility_profile WHERE tenant = $tenant
             AND ($parent = NONE OR profile_key = $parent)
             AND type::string(profile_version) CONTAINS $needle
             GROUP BY profile_version ORDER BY profile_version ASC LIMIT 101 TIMEOUT 5s;
