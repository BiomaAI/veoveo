-- Current owner schema; schema definitions only.

DEFINE TABLE time_acquisition TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE time_active_authority TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE time_authority_release TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE time_calendar_version TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE time_clock_policy TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE time_mission_epoch TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE time_source TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE time_temporal_event TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE FIELD OVERWRITE acquisition_key ON time_acquisition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON time_acquisition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON time_acquisition TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expected_source_digest_sha256 ON time_acquisition TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON time_acquisition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE idempotency_key ON time_acquisition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON time_acquisition TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE phase ON time_acquisition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record_version ON time_acquisition TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_key ON time_acquisition TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE staged_release_key ON time_acquisition TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE status ON time_acquisition TYPE 'queued' | 'running' | 'succeeded' | 'failed' | 'cancel_requested' | 'cancelled' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON time_acquisition TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON time_acquisition TYPE datetime PERMISSIONS FULL;

DEFINE INDEX time_acquisition_idempotency_unique ON time_acquisition FIELDS tenant, owner, idempotency_key UNIQUE;

DEFINE INDEX time_acquisition_staged_release ON time_acquisition FIELDS tenant, staged_release_key;

DEFINE INDEX time_acquisition_tenant_key_unique ON time_acquisition FIELDS tenant, acquisition_key UNIQUE;

DEFINE FIELD OVERWRITE activated_at ON time_active_authority TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE activated_by ON time_active_authority TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dataset_kind ON time_active_authority TYPE 'tzdb' | 'leap_seconds' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON time_active_authority TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE previous_release_key ON time_active_authority TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record_version ON time_active_authority TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE release_key ON time_active_authority TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON time_active_authority TYPE record<tenant> PERMISSIONS FULL;

DEFINE INDEX time_active_authority_tenant_kind_unique ON time_active_authority FIELDS tenant, dataset_kind UNIQUE;

DEFINE FIELD OVERWRITE artifact_path ON time_authority_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON time_authority_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON time_authority_release TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dataset_kind ON time_authority_release TYPE 'tzdb' | 'leap_seconds' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON time_authority_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON time_authority_release TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance ON time_authority_release TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance.owner_key ON time_authority_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance.tenant_key ON time_authority_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance.work_context ON time_authority_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record_version ON time_authority_release TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE release_key ON time_authority_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE retrieved_at ON time_authority_release TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_digest_sha256 ON time_authority_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_key ON time_authority_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_url ON time_authority_release TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON time_authority_release TYPE 'staged' | 'active' | 'retired' | 'quarantined' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON time_authority_release TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON time_authority_release TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE validated_at ON time_authority_release TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE version_label ON time_authority_release TYPE string PERMISSIONS FULL;

DEFINE INDEX time_release_source_digest_unique ON time_authority_release FIELDS tenant, source_key, source_digest_sha256 UNIQUE;

DEFINE INDEX time_release_tenant_key_unique ON time_authority_release FIELDS tenant, release_key UNIQUE;

DEFINE FIELD OVERWRITE calendar_key ON time_calendar_version TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE calendar_version ON time_calendar_version TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE canonical_json ON time_calendar_version TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON time_calendar_version TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON time_calendar_version TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON time_calendar_version TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON time_calendar_version TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance ON time_calendar_version TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance.owner_key ON time_calendar_version TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance.tenant_key ON time_calendar_version TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance.work_context ON time_calendar_version TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON time_calendar_version TYPE 'staged' | 'active' | 'retired' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON time_calendar_version TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON time_calendar_version TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE zone_id ON time_calendar_version TYPE string PERMISSIONS FULL;

DEFINE INDEX time_calendar_tenant_version_unique ON time_calendar_version FIELDS tenant, calendar_key, calendar_version UNIQUE;

DEFINE FIELD OVERWRITE created_at ON time_clock_policy TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON time_clock_policy TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE maximum_error_nanoseconds ON time_clock_policy TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE maximum_holdover_seconds ON time_clock_policy TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE maximum_stratum ON time_clock_policy TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE minimum_source_diversity ON time_clock_policy TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON time_clock_policy TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record_version ON time_clock_policy TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON time_clock_policy TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON time_clock_policy TYPE datetime PERMISSIONS FULL;

DEFINE INDEX time_clock_policy_tenant_unique ON time_clock_policy FIELDS tenant UNIQUE;

DEFINE FIELD OVERWRITE canonical_json ON time_mission_epoch TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON time_mission_epoch TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE epoch_key ON time_mission_epoch TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE epoch_version ON time_mission_epoch TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON time_mission_epoch TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON time_mission_epoch TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE nanosecond ON time_mission_epoch TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON time_mission_epoch TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance ON time_mission_epoch TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance.owner_key ON time_mission_epoch TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance.tenant_key ON time_mission_epoch TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance.work_context ON time_mission_epoch TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tai_seconds_since_1970 ON time_mission_epoch TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON time_mission_epoch TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON time_mission_epoch TYPE datetime PERMISSIONS FULL;

DEFINE INDEX time_epoch_tenant_version_unique ON time_mission_epoch FIELDS tenant, epoch_key, epoch_version UNIQUE;

DEFINE FIELD OVERWRITE canonical_json ON time_source TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON time_source TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE dataset_kind ON time_source TYPE 'tzdb' | 'leap_seconds' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE enabled ON time_source TYPE bool PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expected_content_type ON time_source TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON time_source TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON time_source TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON time_source TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record_version ON time_source TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_key ON time_source TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_url ON time_source TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON time_source TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON time_source TYPE datetime PERMISSIONS FULL;

DEFINE INDEX time_source_tenant_key_unique ON time_source FIELDS tenant, source_key UNIQUE;

DEFINE FIELD OVERWRITE canonical_json ON time_temporal_event TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON time_temporal_event TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE due_nanosecond ON time_temporal_event TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE due_tai_seconds_since_1970 ON time_temporal_event TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE event_key ON time_temporal_event TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON time_temporal_event TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE idempotency_key ON time_temporal_event TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE name ON time_temporal_event TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON time_temporal_event TYPE record<principal> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance ON time_temporal_event TYPE object PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance.owner_key ON time_temporal_event TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance.tenant_key ON time_temporal_event TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provenance.work_context ON time_temporal_event TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE record_version ON time_temporal_event TYPE int PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON time_temporal_event TYPE 'scheduled' | 'due' | 'cancelled' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON time_temporal_event TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON time_temporal_event TYPE datetime PERMISSIONS FULL;

DEFINE INDEX time_event_due ON time_temporal_event FIELDS tenant, state, due_tai_seconds_since_1970, due_nanosecond;

DEFINE INDEX time_event_idempotency_unique ON time_temporal_event FIELDS tenant, owner, idempotency_key UNIQUE;

DEFINE INDEX time_event_tenant_key_unique ON time_temporal_event FIELDS tenant, event_key UNIQUE;
