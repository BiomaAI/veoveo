UPDATE simulation_view_state UNSET snapshot.leases;

DEFINE FIELD OVERWRITE desired_digest_schema ON TABLE simulation_view_state TYPE
  "veoveo.io/simulation-view-desired-digest/legacy-v1"
  | "veoveo.io/simulation-view-desired-digest/v1"
  | "veoveo.io/simulation-view-desired-digest/v2";
