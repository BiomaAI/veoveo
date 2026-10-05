SELECT * FROM artifact_access_request WHERE tenant = $tenant AND requester = $scope ORDER BY id DESC LIMIT $limit;
