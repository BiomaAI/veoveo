-- Existing plans keep an explicit read profile; every new admission creates its Task link.
DEFINE FIELD IF NOT EXISTS execution_profile ON TABLE uav_vehicle_mission_plan TYPE 'legacy_v1' | 'task_linked_v1'
  DEFAULT 'legacy_v1';
UPDATE uav_vehicle_mission_plan SET
  execution_profile = 'legacy_v1'
WHERE execution_profile = NONE;

DEFINE TABLE IF NOT EXISTS uav_mission_execution SCHEMAFULL TYPE NORMAL
  CHANGEFEED 30d INCLUDE ORIGINAL
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
