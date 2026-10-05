SELECT * FROM artifact_access_request WHERE tenant = $tenant AND work_context = $scope AND state = $state AND id < $cursor ORDER BY id DESC LIMIT $limit;
