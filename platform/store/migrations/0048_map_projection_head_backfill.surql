-- The index created in 0047 is not populated until that migration commits.
-- Read canonical rows directly when backfilling existing installations.
LET $latest_map_commit = array::first((SELECT VALUE commit_sequence FROM map_feature_changeset WITH NOINDEX ORDER BY commit_sequence DESC LIMIT 1)) ?? 0;
UPDATE ONLY map_projection_state:authored_features SET last_sequence = math::max([last_sequence, $latest_map_commit]);
