DEFINE FIELD IF NOT EXISTS desired_digest ON TABLE simulation_view_state TYPE option<string>;
DEFINE FIELD IF NOT EXISTS desired_digest_schema ON TABLE simulation_view_state TYPE option<string>;

UPDATE simulation_view_state SET
  desired_digest = snapshot_digest,
  desired_digest_schema = "veoveo.io/simulation-view-desired-digest/legacy-v1";

REMOVE FIELD IF EXISTS snapshot_digest ON TABLE simulation_view_state;
UPDATE simulation_view_state UNSET snapshot_digest;

DEFINE FIELD OVERWRITE desired_digest ON TABLE simulation_view_state TYPE string
  ASSERT $value.len() = 64;
DEFINE FIELD OVERWRITE desired_digest_schema ON TABLE simulation_view_state TYPE
  "veoveo.io/simulation-view-desired-digest/legacy-v1"
  | "veoveo.io/simulation-view-desired-digest/v1";
