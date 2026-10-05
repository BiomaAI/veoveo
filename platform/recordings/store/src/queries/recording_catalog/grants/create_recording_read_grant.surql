BEGIN TRANSACTION;
             IF !($request.dataset.tenant = $scope.tenant
    AND array::len((SELECT VALUE id FROM $request.recordings
        WHERE tenant = $scope.tenant AND dataset = $request.dataset
          AND $scope.labels CONTAINSALL labels)) = array::len($request.recordings)) OR $expires <= time::now()
                OR $expires > time::now() + 1h {
                 THROW 'recording_read_grant_admission_denied';
             };
             CREATE ONLY $grant SET tenant = $scope.tenant, actor = $scope.actor,
                 work_context = $scope.context, policy_revision = $scope.policy,
                 dataset = $request.dataset, grant_class = $request.class,
                 recordings = $request.recordings, catalog_revision = $request.revision,
                 admitted_set_digest = $request.digest, expires_at = $expires,
                 created_at = time::now();
             COMMIT TRANSACTION;
