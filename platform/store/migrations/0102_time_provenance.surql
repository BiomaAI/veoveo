DEFINE FIELD provenance ON time_authority_release TYPE object;
DEFINE FIELD provenance.tenant_key ON time_authority_release TYPE string;
DEFINE FIELD provenance.owner_key ON time_authority_release TYPE string;
DEFINE FIELD provenance.work_context ON time_authority_release TYPE string;

DEFINE FIELD provenance ON time_calendar_version TYPE object;
DEFINE FIELD provenance.tenant_key ON time_calendar_version TYPE string;
DEFINE FIELD provenance.owner_key ON time_calendar_version TYPE string;
DEFINE FIELD provenance.work_context ON time_calendar_version TYPE string;

DEFINE FIELD provenance ON time_mission_epoch TYPE object;
DEFINE FIELD provenance.tenant_key ON time_mission_epoch TYPE string;
DEFINE FIELD provenance.owner_key ON time_mission_epoch TYPE string;
DEFINE FIELD provenance.work_context ON time_mission_epoch TYPE string;

DEFINE FIELD provenance ON time_temporal_event TYPE object;
DEFINE FIELD provenance.tenant_key ON time_temporal_event TYPE string;
DEFINE FIELD provenance.owner_key ON time_temporal_event TYPE string;
DEFINE FIELD provenance.work_context ON time_temporal_event TYPE string;
