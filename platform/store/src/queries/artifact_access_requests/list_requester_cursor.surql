SELECT * FROM artifact_access_request WHERE tenant = $tenant AND requester = $scope AND id < $cursor ORDER BY id DESC LIMIT $limit;
