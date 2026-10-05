SELECT VALUE <string> record::id(generation) FROM ONLY $active WHERE generation.tenant = $tenant AND generation.state = 'active';
