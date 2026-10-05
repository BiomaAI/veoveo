BEGIN TRANSACTION;
             LET $existing = (SELECT VALUE id FROM recording_projection_receipt
    WHERE tenant = $scope.tenant AND actor = $scope.actor
      AND caller_idempotency_key = $request.key LIMIT 1)[0];
             LET $result = IF $existing != NONE {
                 LET $row = (SELECT * FROM ONLY $existing WHERE tenant = $scope.tenant AND actor = $scope.actor
    AND work_context = $scope.context AND policy_revision = $scope.policy
    AND recordings = [$recording] AND expires_at > time::now()
    AND $recording.tenant = $scope.tenant AND $scope.labels CONTAINSALL $recording.labels
    AND dataset = $recording.dataset AND dataset.tenant = $scope.tenant
    AND grant.tenant = $scope.tenant AND grant.actor = $scope.actor
    AND grant.work_context = $scope.context AND grant.policy_revision = $scope.policy
    AND grant.grant_class = $grant_class AND grant.dataset = dataset
    AND grant.recordings = recordings AND grant.catalog_revision = catalog_revision
    AND grant.expires_at >= expires_at AND grant.expires_at > time::now() AND dataset = $request.dataset AND manifest_digest = $request.manifest
    AND query_digest = $request.query);
                 IF $row = NONE { THROW 'recording_projection_request_conflict'; };
                 $row
             } ELSE {
                 LET $admitted = (SELECT * FROM ONLY $grant WHERE tenant = $scope.tenant AND actor = $scope.actor
    AND work_context = $scope.context AND policy_revision = $scope.policy
    AND grant_class = $grant_class AND recordings = [$recording]
    AND dataset = $request.dataset AND dataset = $recording.dataset
    AND dataset.tenant = $scope.tenant AND $recording.tenant = $scope.tenant
    AND $scope.labels CONTAINSALL $recording.labels
    AND expires_at >= $expires AND $expires > time::now());
                 IF $admitted = NONE { THROW 'recording_projection_grant_conflict'; };
                 CREATE ONLY $projection SET
                     tenant = $scope.tenant, actor = $scope.actor,
                     work_context = $scope.context, policy_revision = $scope.policy,
                     grant = $grant, dataset = $request.dataset, recordings = [$recording],
                     catalog_revision = $admitted.catalog_revision,
                     caller_idempotency_key = $request.key,
                     manifest_digest = $request.manifest, query_digest = $request.query,
                     state = $reserved, expires_at = $expires,
                     created_at = time::now(), updated_at = time::now()
             };
             RETURN $result;
             COMMIT TRANSACTION;
