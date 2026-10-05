SELECT * FROM artifact_access_request WHERE tenant = $tenant AND artifact = $artifact AND requester = $requester LIMIT 1;
