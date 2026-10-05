SELECT * FROM map_mobility_profile WHERE tenant = $tenant AND (profile_key > $key OR (profile_key = $key AND profile_version > $version))
             ORDER BY profile_key ASC, profile_version ASC LIMIT $limit TIMEOUT 5s;
