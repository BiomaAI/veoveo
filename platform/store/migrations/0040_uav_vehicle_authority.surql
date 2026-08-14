-- UAV Simulation-owned principal-to-vehicle grants, admitted mission plans,
-- and exclusive command leases. The platform store supplies persistence; the
-- UAV Simulation MCP server owns every policy and lifecycle transition.

DEFINE TABLE IF NOT EXISTS uav_vehicle_control_grant SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE uav_vehicle_control_grant TYPE string;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE uav_vehicle_control_grant TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS work_context ON TABLE uav_vehicle_control_grant TYPE record<work_context>;
DEFINE FIELD IF NOT EXISTS grant_id ON TABLE uav_vehicle_control_grant TYPE string;
DEFINE FIELD IF NOT EXISTS session_id ON TABLE uav_vehicle_control_grant TYPE string;
DEFINE FIELD IF NOT EXISTS vehicle_id ON TABLE uav_vehicle_control_grant TYPE string;
DEFINE FIELD IF NOT EXISTS principal_key ON TABLE uav_vehicle_control_grant TYPE string;
DEFINE FIELD IF NOT EXISTS permissions ON TABLE uav_vehicle_control_grant TYPE array<"inspect" | "plan" | "execute" | "abort">;
DEFINE FIELD IF NOT EXISTS map_mobility_profile_uri ON TABLE uav_vehicle_control_grant TYPE string;
DEFINE FIELD IF NOT EXISTS allow_planning_advisory ON TABLE uav_vehicle_control_grant TYPE bool DEFAULT false;
DEFINE FIELD IF NOT EXISTS valid_from ON TABLE uav_vehicle_control_grant TYPE datetime;
DEFINE FIELD IF NOT EXISTS valid_until ON TABLE uav_vehicle_control_grant TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS created_by ON TABLE uav_vehicle_control_grant TYPE string;
DEFINE FIELD IF NOT EXISTS revoked_at ON TABLE uav_vehicle_control_grant TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS revoked_by ON TABLE uav_vehicle_control_grant TYPE option<string>;
DEFINE FIELD IF NOT EXISTS revision ON TABLE uav_vehicle_control_grant TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE uav_vehicle_control_grant TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE uav_vehicle_control_grant TYPE datetime;
DEFINE INDEX IF NOT EXISTS uav_control_grant_context_id_unique ON TABLE uav_vehicle_control_grant FIELDS tenant, work_context, grant_id UNIQUE;
DEFINE INDEX IF NOT EXISTS uav_control_grant_principal_vehicle ON TABLE uav_vehicle_control_grant FIELDS tenant, work_context, principal_key, session_id, vehicle_id;

DEFINE TABLE IF NOT EXISTS uav_vehicle_mission_plan SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE uav_vehicle_mission_plan TYPE string;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE uav_vehicle_mission_plan TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS work_context ON TABLE uav_vehicle_mission_plan TYPE record<work_context>;
DEFINE FIELD IF NOT EXISTS plan_id ON TABLE uav_vehicle_mission_plan TYPE string;
DEFINE FIELD IF NOT EXISTS mission_id ON TABLE uav_vehicle_mission_plan TYPE string;
DEFINE FIELD IF NOT EXISTS principal_key ON TABLE uav_vehicle_mission_plan TYPE string;
DEFINE FIELD IF NOT EXISTS session_id ON TABLE uav_vehicle_mission_plan TYPE string;
DEFINE FIELD IF NOT EXISTS vehicle_id ON TABLE uav_vehicle_mission_plan TYPE string;
DEFINE FIELD IF NOT EXISTS map_route_uri ON TABLE uav_vehicle_mission_plan TYPE string;
DEFINE FIELD IF NOT EXISTS map_route_digest_sha256 ON TABLE uav_vehicle_mission_plan TYPE string;
DEFINE FIELD IF NOT EXISTS map_mobility_profile_uri ON TABLE uav_vehicle_mission_plan TYPE string;
DEFINE FIELD IF NOT EXISTS state ON TABLE uav_vehicle_mission_plan TYPE "prepared" | "executing" | "completed" | "failed" | "cancelled";
DEFINE FIELD IF NOT EXISTS canonical_json ON TABLE uav_vehicle_mission_plan TYPE string;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE uav_vehicle_mission_plan TYPE datetime;
DEFINE FIELD IF NOT EXISTS revision ON TABLE uav_vehicle_mission_plan TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE uav_vehicle_mission_plan TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE uav_vehicle_mission_plan TYPE datetime;
DEFINE INDEX IF NOT EXISTS uav_mission_plan_context_id_unique ON TABLE uav_vehicle_mission_plan FIELDS tenant, work_context, plan_id UNIQUE;
DEFINE INDEX IF NOT EXISTS uav_mission_plan_principal_time ON TABLE uav_vehicle_mission_plan FIELDS tenant, work_context, principal_key, created_at;
DEFINE INDEX IF NOT EXISTS uav_mission_plan_route_digest ON TABLE uav_vehicle_mission_plan FIELDS tenant, work_context, map_route_digest_sha256;

DEFINE TABLE IF NOT EXISTS uav_vehicle_command_lease SCHEMAFULL TYPE NORMAL CHANGEFEED 30d INCLUDE ORIGINAL PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE uav_vehicle_command_lease TYPE string;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE uav_vehicle_command_lease TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS work_context ON TABLE uav_vehicle_command_lease TYPE record<work_context>;
DEFINE FIELD IF NOT EXISTS session_id ON TABLE uav_vehicle_command_lease TYPE string;
DEFINE FIELD IF NOT EXISTS vehicle_id ON TABLE uav_vehicle_command_lease TYPE string;
DEFINE FIELD IF NOT EXISTS principal_key ON TABLE uav_vehicle_command_lease TYPE string;
DEFINE FIELD IF NOT EXISTS mission_id ON TABLE uav_vehicle_command_lease TYPE string;
DEFINE FIELD IF NOT EXISTS lease_token ON TABLE uav_vehicle_command_lease TYPE string;
DEFINE FIELD IF NOT EXISTS expires_at ON TABLE uav_vehicle_command_lease TYPE datetime;
DEFINE FIELD IF NOT EXISTS released_at ON TABLE uav_vehicle_command_lease TYPE option<datetime>;
DEFINE FIELD IF NOT EXISTS revision ON TABLE uav_vehicle_command_lease TYPE int DEFAULT 0 ASSERT $value >= 0;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE uav_vehicle_command_lease TYPE datetime;
DEFINE FIELD IF NOT EXISTS updated_at ON TABLE uav_vehicle_command_lease TYPE datetime;
DEFINE INDEX IF NOT EXISTS uav_command_lease_vehicle_unique ON TABLE uav_vehicle_command_lease FIELDS tenant, work_context, session_id, vehicle_id UNIQUE;
DEFINE INDEX IF NOT EXISTS uav_command_lease_expiry ON TABLE uav_vehicle_command_lease FIELDS tenant, work_context, expires_at;
