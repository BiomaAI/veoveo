-- Map recovery pages its immutable domain log rather than the shared outbox.
DEFINE INDEX IF NOT EXISTS map_feature_changeset_commit_sequence ON TABLE map_feature_changeset FIELDS commit_sequence;

-- The sequence allocator is independent of the data transaction. A shared Map
-- head serializes accepted commits and supplies a committed recovery boundary.
DEFINE TABLE IF NOT EXISTS map_projection_state SCHEMAFULL TYPE NORMAL CHANGEFEED 90d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE map_projection_state TYPE string;
DEFINE FIELD IF NOT EXISTS last_sequence ON TABLE map_projection_state TYPE int ASSERT $value >= 0;
CREATE ONLY map_projection_state:authored_features SET last_sequence = array::first((SELECT VALUE commit_sequence FROM map_feature_changeset ORDER BY commit_sequence DESC LIMIT 1)) ?? 0;

DEFINE EVENT IF NOT EXISTS map_feature_changeset_projection_sequence ON TABLE map_feature_changeset
WHEN $after.commit_sequence > 0 AND $before.commit_sequence != $after.commit_sequence
THEN {
    LET $head = (SELECT * FROM ONLY map_projection_state:authored_features);
    IF $head = NONE OR $after.commit_sequence <= $head.last_sequence {
        THROW 'map_feature_projection_sequence_conflict';
    };
    UPDATE ONLY map_projection_state:authored_features SET last_sequence = $after.commit_sequence;
};
