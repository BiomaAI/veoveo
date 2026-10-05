SELECT * FROM artifact_access_request WHERE tenant = $tenant AND work_context = $scope ORDER BY id DESC LIMIT $limit;
