DEFINE TABLE stream_run SCHEMAFULL CHANGEFEED 1d INCLUDE ORIGINAL;
DEFINE FIELD id ON stream_run TYPE string;
DEFINE FIELD task ON stream_run TYPE record<task> REFERENCE ON DELETE CASCADE;
DEFINE FIELD task_type ON stream_run TYPE string;
DEFINE FIELD created_at ON stream_run TYPE datetime;
DEFINE FIELD identity ON stream_run TYPE object ASSERT object::keys($value) ALLINSIDE ['pipeline_id', 'tenant'] AND ['pipeline_id', 'tenant'] ALLINSIDE object::keys($value);
DEFINE FIELD identity.pipeline_id ON stream_run TYPE string;
DEFINE FIELD settlement ON stream_run TYPE option<object> ASSERT IF type::is_object($value) THEN object::keys($value) ALLINSIDE ['status', 'outcome', 'completed_at', 'results', 'annotations', 'source_clip'] AND $value.status != NONE AND $value.completed_at != NONE AND (($value.status = 'succeeded' AND $value.outcome = 'product' AND $value.results != NONE AND $value.annotations != NONE) OR ((($value.status = 'succeeded' AND $value.outcome = 'tool_error') OR ($value.status = 'failed' AND $value.outcome = 'failed') OR ($value.status = 'cancelled' AND $value.outcome = 'cancelled')) AND $value.results = NONE AND $value.annotations = NONE AND $value.source_clip = NONE)) ELSE $value = NONE END;
DEFINE FIELD settlement.status ON stream_run TYPE option<string> ASSERT $value = NONE OR $value IN ['succeeded', 'failed', 'cancelled'];
DEFINE FIELD settlement.completed_at ON stream_run TYPE option<datetime>;
DEFINE FIELD settlement.results ON stream_run TYPE option<record<artifact_occurrence>>;
DEFINE FIELD settlement.annotations ON stream_run TYPE option<record<artifact_occurrence>>;
DEFINE FIELD settlement.source_clip ON stream_run TYPE option<record<artifact_occurrence>>;

DEFINE FIELD identity.tenant ON stream_run TYPE record<tenant>;
DEFINE INDEX stream_run_task ON stream_run FIELDS task UNIQUE;
DEFINE INDEX stream_run_page ON stream_run FIELDS identity.tenant, created_at, task;
DEFINE FIELD settlement.outcome ON stream_run TYPE option<string> ASSERT $value = NONE OR $value IN ['product', 'tool_error', 'failed', 'cancelled'];
