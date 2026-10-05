
DEFINE TABLE measurement_state SCHEMAFULL CHANGEFEED 30d INCLUDE ORIGINAL;
DEFINE FIELD revision ON measurement_state TYPE int;
DEFINE SEQUENCE measurement_sequence BATCH 1 START 1;
DEFINE TABLE measurement_event SCHEMAFULL CHANGEFEED 30d INCLUDE ORIGINAL;
DEFINE FIELD sequence ON measurement_event TYPE int DEFAULT sequence::nextval('measurement_sequence') READONLY;
DEFINE FIELD tenant ON measurement_event TYPE option<record<tenant>>;
DEFINE FIELD aggregate_type ON measurement_event TYPE string;
DEFINE FIELD aggregate_id ON measurement_event TYPE string;
DEFINE FIELD event_type ON measurement_event TYPE string;
DEFINE FIELD schema_version ON measurement_event TYPE int ASSERT $value > 0;
DEFINE FIELD payload ON measurement_event TYPE object FLEXIBLE;
DEFINE FIELD occurred_at ON measurement_event TYPE datetime DEFAULT time::now();
DEFINE FIELD available_at ON measurement_event TYPE datetime DEFAULT time::now();
DEFINE INDEX measurement_event_sequence ON measurement_event FIELDS sequence UNIQUE;
DEFINE INDEX measurement_event_available ON measurement_event FIELDS available_at, sequence;
DEFINE INDEX measurement_event_aggregate ON measurement_event FIELDS aggregate_type, aggregate_id, sequence;
DEFINE INDEX measurement_event_tenant ON measurement_event FIELDS tenant, sequence;
