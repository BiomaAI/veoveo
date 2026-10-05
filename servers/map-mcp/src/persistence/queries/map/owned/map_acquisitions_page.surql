SELECT * FROM map_acquisition WHERE tenant = $tenant AND owner = $owner AND ($after = NONE OR acquisition_key > $after) ORDER BY acquisition_key ASC LIMIT $limit;
