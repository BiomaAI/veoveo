SELECT * FROM principal WHERE tenant = $tenant AND id = $owner; SELECT * FROM share_link WHERE tenant = $tenant AND artifact = $artifact LIMIT 200;
