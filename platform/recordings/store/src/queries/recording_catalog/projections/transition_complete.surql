BEGIN TRANSACTION;
LET $current = (SELECT * FROM ONLY $projection WHERE tenant = $scope.tenant AND actor = $scope.actor
    AND work_context = $scope.context AND policy_revision = $scope.policy
    AND recordings = [$recording] AND expires_at > time::now()
    AND $recording.tenant = $scope.tenant AND $scope.labels CONTAINSALL $recording.labels
    AND dataset = $recording.dataset AND dataset.tenant = $scope.tenant
    AND grant.tenant = $scope.tenant AND grant.actor = $scope.actor
    AND grant.work_context = $scope.context AND grant.policy_revision = $scope.policy
    AND grant.grant_class = $grant_class AND grant.dataset = dataset
    AND grant.recordings = recordings AND grant.catalog_revision = catalog_revision
    AND grant.expires_at >= expires_at AND grant.expires_at > time::now()
    AND (state IN $predecessors OR (state = $target AND result_byte_len = $bytes AND result_sha256 = $digest)));
IF $current = NONE { THROW 'recording_projection_transition_conflict'; };
IF $current.state != $target {
    UPDATE ONLY $projection SET state = $target, result_byte_len = $bytes, result_sha256 = $digest, failure_reason = NONE, updated_at = time::now() RETURN NONE;
};
RETURN (SELECT * FROM ONLY $projection);
COMMIT TRANSACTION;
