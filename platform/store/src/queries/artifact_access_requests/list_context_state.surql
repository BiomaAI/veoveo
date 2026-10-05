SELECT * FROM artifact_access_request WHERE tenant = $tenant AND work_context = $scope AND state = $state ORDER BY id DESC LIMIT $limit;
