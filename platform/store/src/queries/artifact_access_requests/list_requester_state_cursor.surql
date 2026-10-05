SELECT * FROM artifact_access_request WHERE tenant = $tenant AND requester = $scope AND state = $state AND id < $cursor ORDER BY id DESC LIMIT $limit;
