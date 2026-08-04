UPDATE simulation_view_state UNSET snapshot.leases;

DEFINE FIELD IF NOT EXISTS snapshot.leases ON TABLE simulation_view_state TYPE none;
