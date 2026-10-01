-- Mission admission creates one exact Task link in the plan/lease transaction.
DEFINE TABLE IF NOT EXISTS uav_mission_execution SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d
  PERMISSIONS NONE;
DEFINE FIELD IF NOT EXISTS id ON TABLE uav_mission_execution TYPE uuid;
DEFINE FIELD IF NOT EXISTS task ON TABLE uav_mission_execution TYPE record<task>;
DEFINE FIELD IF NOT EXISTS plan ON TABLE uav_mission_execution TYPE record<uav_vehicle_mission_plan>;
DEFINE FIELD IF NOT EXISTS tenant ON TABLE uav_mission_execution TYPE record<tenant>;
DEFINE FIELD IF NOT EXISTS work_context ON TABLE uav_mission_execution TYPE record<work_context>;
DEFINE FIELD IF NOT EXISTS principal_key ON TABLE uav_mission_execution TYPE string;
DEFINE FIELD IF NOT EXISTS lease ON TABLE uav_mission_execution TYPE record<uav_vehicle_command_lease>;
DEFINE FIELD IF NOT EXISTS lease_token ON TABLE uav_mission_execution TYPE string;
DEFINE FIELD IF NOT EXISTS created_at ON TABLE uav_mission_execution TYPE datetime;
DEFINE INDEX IF NOT EXISTS uav_execution_plan ON TABLE uav_mission_execution FIELDS plan
  UNIQUE;
DEFINE INDEX IF NOT EXISTS uav_execution_task ON TABLE uav_mission_execution FIELDS task
  UNIQUE;
