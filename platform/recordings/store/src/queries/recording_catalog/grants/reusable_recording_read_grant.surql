SELECT * FROM ONLY $grant WHERE tenant = $scope.tenant AND actor = $scope.actor
                AND work_context = $scope.context AND policy_revision = $scope.policy
                AND dataset = $request.dataset AND grant_class = $request.class
                AND recordings = $request.recordings AND catalog_revision = $request.revision
                AND admitted_set_digest = $request.digest AND expires_at > time::now()
                AND $request.dataset.tenant = $scope.tenant
    AND array::len((SELECT VALUE id FROM $request.recordings
        WHERE tenant = $scope.tenant AND dataset = $request.dataset
          AND $scope.labels CONTAINSALL labels)) = array::len($request.recordings);
