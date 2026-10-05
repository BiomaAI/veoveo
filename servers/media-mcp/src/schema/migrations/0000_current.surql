-- Current owner schema; schema definitions only.

DEFINE TABLE media_task_context TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE TABLE media_usage TYPE NORMAL SCHEMAFULL CHANGEFEED 4w2d PERMISSIONS NONE;

DEFINE FIELD OVERWRITE capability ON media_task_context TYPE record<artifact_write_capability> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE capability_expires_at ON media_task_context TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE capability_secret ON media_task_context TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON media_task_context TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON media_task_context TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON media_task_context TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON media_task_context TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE updated_at ON media_task_context TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE INDEX media_task_context_expiry ON media_task_context FIELDS capability_expires_at;

DEFINE INDEX media_task_context_task ON media_task_context FIELDS task UNIQUE;

DEFINE FIELD OVERWRITE amount ON media_usage TYPE none | float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE currency ON media_usage TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON media_usage TYPE uuid DEFAULT `rand`::uuid() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE kind ON media_usage TYPE 'estimate' | 'actual' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE metadata ON media_usage TYPE object FLEXIBLE DEFAULT {  } PERMISSIONS FULL;

DEFINE FIELD OVERWRITE model_id ON media_usage TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE provider_job ON media_usage TYPE none | record<provider_job> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE quantity ON media_usage TYPE none | float PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recorded_at ON media_usage TYPE datetime DEFAULT time::now() PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_id ON media_usage TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE task ON media_usage TYPE record<task> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON media_usage TYPE record<tenant> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE unit ON media_usage TYPE none | string PERMISSIONS FULL;

DEFINE INDEX media_usage_retention ON media_usage FIELDS recorded_at;

DEFINE INDEX media_usage_source_unique ON media_usage FIELDS task, kind, source_id UNIQUE;

DEFINE INDEX media_usage_task_time ON media_usage FIELDS task, recorded_at;
