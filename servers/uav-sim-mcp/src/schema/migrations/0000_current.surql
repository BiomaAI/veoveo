-- Current owner schema; schema definitions only.

DEFINE TABLE uav_mission_execution TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE uav_vehicle_command_lease TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE uav_vehicle_control_grant TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE uav_vehicle_mission_plan TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE FIELD OVERWRITE created_at ON uav_mission_execution TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON uav_mission_execution TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease ON uav_mission_execution TYPE record<uav_vehicle_command_lease> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_token ON uav_mission_execution TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE plan ON uav_mission_execution TYPE record<uav_vehicle_mission_plan> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE principal_key ON uav_mission_execution TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON uav_mission_execution TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON uav_mission_execution TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON uav_mission_execution TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX uav_execution_plan ON uav_mission_execution FIELDS plan UNIQUE;

DEFINE INDEX uav_execution_task ON uav_mission_execution FIELDS task UNIQUE;

DEFINE FIELD OVERWRITE created_at ON uav_vehicle_command_lease TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON uav_vehicle_command_lease TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON uav_vehicle_command_lease TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE lease_token ON uav_vehicle_command_lease TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE mission_id ON uav_vehicle_command_lease TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE principal_key ON uav_vehicle_command_lease TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE released_at ON uav_vehicle_command_lease TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON uav_vehicle_command_lease TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE session_id ON uav_vehicle_command_lease TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON uav_vehicle_command_lease TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON uav_vehicle_command_lease TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE vehicle_id ON uav_vehicle_command_lease TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON uav_vehicle_command_lease TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX uav_command_lease_expiry ON uav_vehicle_command_lease FIELDS tenant, work_context, expires_at;

DEFINE INDEX uav_command_lease_vehicle_unique ON uav_vehicle_command_lease FIELDS tenant, work_context, session_id, vehicle_id UNIQUE;

DEFINE FIELD OVERWRITE allow_planning_advisory ON uav_vehicle_control_grant TYPE bool DEFAULT false PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON uav_vehicle_control_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_by ON uav_vehicle_control_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE grant_id ON uav_vehicle_control_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON uav_vehicle_control_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE map_mobility_profile_uri ON uav_vehicle_control_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE permissions ON uav_vehicle_control_grant TYPE array<'inspect' | 'plan' | 'execute' | 'abort'> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE permissions.* ON uav_vehicle_control_grant TYPE 'inspect' | 'plan' | 'execute' | 'abort' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE principal_key ON uav_vehicle_control_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON uav_vehicle_control_grant TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revoked_at ON uav_vehicle_control_grant TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revoked_by ON uav_vehicle_control_grant TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE session_id ON uav_vehicle_control_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON uav_vehicle_control_grant TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON uav_vehicle_control_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_from ON uav_vehicle_control_grant TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_until ON uav_vehicle_control_grant TYPE none | datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE vehicle_id ON uav_vehicle_control_grant TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON uav_vehicle_control_grant TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX uav_control_grant_context_id_unique ON uav_vehicle_control_grant FIELDS tenant, work_context, grant_id UNIQUE;

DEFINE INDEX uav_control_grant_principal_vehicle ON uav_vehicle_control_grant FIELDS tenant, work_context, principal_key, session_id, vehicle_id;

DEFINE FIELD OVERWRITE canonical_json ON uav_vehicle_mission_plan TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON uav_vehicle_mission_plan TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON uav_vehicle_mission_plan TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON uav_vehicle_mission_plan TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE map_mobility_profile_uri ON uav_vehicle_mission_plan TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE map_route_digest_sha256 ON uav_vehicle_mission_plan TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE map_route_uri ON uav_vehicle_mission_plan TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE mission_id ON uav_vehicle_mission_plan TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE plan_id ON uav_vehicle_mission_plan TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE principal_key ON uav_vehicle_mission_plan TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON uav_vehicle_mission_plan TYPE int DEFAULT 0 ASSERT $value >= 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE session_id ON uav_vehicle_mission_plan TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON uav_vehicle_mission_plan TYPE 'prepared' | 'executing' | 'completed' | 'failed' | 'cancelled' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON uav_vehicle_mission_plan TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON uav_vehicle_mission_plan TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE vehicle_id ON uav_vehicle_mission_plan TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE work_context ON uav_vehicle_mission_plan TYPE record<work_context> PERMISSIONS FULL;

DEFINE INDEX uav_mission_plan_context_id_unique ON uav_vehicle_mission_plan FIELDS tenant, work_context, plan_id UNIQUE;

DEFINE INDEX uav_mission_plan_mission ON uav_vehicle_mission_plan FIELDS tenant, work_context, principal_key, mission_id;

DEFINE INDEX uav_mission_plan_principal_time ON uav_vehicle_mission_plan FIELDS tenant, work_context, principal_key, created_at;

DEFINE INDEX uav_mission_plan_route_digest ON uav_vehicle_mission_plan FIELDS tenant, work_context, map_route_digest_sha256;

DEFINE INDEX uav_mission_plan_vehicle_state ON uav_vehicle_mission_plan FIELDS tenant, work_context, session_id, vehicle_id, state;
