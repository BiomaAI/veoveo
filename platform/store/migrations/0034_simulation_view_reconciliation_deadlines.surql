UPDATE simulation_view_state SET
  reconciliation.retryAt = IF reconciliation.phase = "blocked" THEN reconciliation.nextAttemptAt ELSE NONE END,
  reconciliation.poseAuthorizationRenewalAt = IF reconciliation.phase != "blocked" THEN reconciliation.nextAttemptAt ELSE NONE END,
  snapshot.session.reconciliation.retryAt = IF snapshot.session.reconciliation.phase = "blocked" THEN snapshot.session.reconciliation.nextAttemptAt ELSE NONE END,
  snapshot.session.reconciliation.poseAuthorizationRenewalAt = IF snapshot.session.reconciliation.phase != "blocked" THEN snapshot.session.reconciliation.nextAttemptAt ELSE NONE END;

UPDATE simulation_view_state SET
  reconciliation.phase = IF reconciliation.phase = "streams" THEN "pending" ELSE reconciliation.phase END,
  snapshot.session.reconciliation.phase = IF snapshot.session.reconciliation.phase = "streams" THEN "pending" ELSE snapshot.session.reconciliation.phase END,
  snapshot.session.lifecycle = IF snapshot.session.lifecycle = "failed" THEN
    IF snapshot.session.scene = NONE THEN "created" ELSE "scene_bound" END
  ELSE snapshot.session.lifecycle END;

UPDATE simulation_view_state SET
  reconciliation.phase = "pending",
  reconciliation.renewalState = "scheduled",
  reconciliation.retryAt = NONE,
  reconciliation.failedDependency = NONE,
  reconciliation.diagnostic = NONE
WHERE reconciliation.failureCode = "audit_unavailable";

UPDATE simulation_view_state SET
  snapshot.session.reconciliation.phase = "pending",
  snapshot.session.reconciliation.renewalState = "scheduled",
  snapshot.session.reconciliation.retryAt = NONE,
  snapshot.session.reconciliation.failedDependency = NONE,
  snapshot.session.reconciliation.diagnostic = NONE
WHERE snapshot.session.reconciliation.failureCode = "audit_unavailable";

UPDATE simulation_view_state SET
  reconciliation.failureCode = "renderer_unavailable"
WHERE reconciliation.failureCode = "stream_unavailable";

UPDATE simulation_view_state SET
  snapshot.session.reconciliation.failureCode = "renderer_unavailable"
WHERE snapshot.session.reconciliation.failureCode = "stream_unavailable";

UPDATE simulation_view_state SET
  reconciliation.phase = "revoked",
  reconciliation.renewalState = "revoked",
  reconciliation.retryAt = NONE,
  reconciliation.failedDependency = NONE,
  reconciliation.diagnostic = NONE
WHERE reconciliation.failureCode = "producer_revoked";

UPDATE simulation_view_state SET
  snapshot.session.reconciliation.phase = "revoked",
  snapshot.session.reconciliation.renewalState = "revoked",
  snapshot.session.reconciliation.retryAt = NONE,
  snapshot.session.reconciliation.failedDependency = NONE,
  snapshot.session.reconciliation.diagnostic = NONE
WHERE snapshot.session.reconciliation.failureCode = "producer_revoked";

UPDATE simulation_view_state UNSET
  reconciliation.nextAttemptAt,
  snapshot.session.reconciliation.nextAttemptAt;

UPDATE simulation_view_state UNSET reconciliation.failureCode
WHERE reconciliation.failureCode = "audit_unavailable"
  OR reconciliation.failureCode = "producer_revoked";

UPDATE simulation_view_state UNSET snapshot.session.reconciliation.failureCode
WHERE snapshot.session.reconciliation.failureCode = "audit_unavailable"
  OR snapshot.session.reconciliation.failureCode = "producer_revoked";
