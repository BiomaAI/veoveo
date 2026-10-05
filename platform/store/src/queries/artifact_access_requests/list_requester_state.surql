SELECT * FROM artifact_access_request WHERE tenant = $tenant AND requester = $scope AND state = $state ORDER BY id DESC LIMIT $limit;
