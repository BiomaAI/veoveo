DEFINE TABLE knowledge_collection SCHEMAFULL CHANGEFEED 7d PERMISSIONS NONE;
DEFINE FIELD id ON knowledge_collection TYPE array;
DEFINE FIELD tenant ON knowledge_collection TYPE string;
DEFINE FIELD collection ON knowledge_collection TYPE string;
DEFINE FIELD revision ON knowledge_collection TYPE string;
DEFINE FIELD approved ON knowledge_collection TYPE bool;
DEFINE FIELD document ON knowledge_collection TYPE object FLEXIBLE;
DEFINE INDEX knowledge_collection_scope ON knowledge_collection FIELDS tenant, collection
    UNIQUE;

DEFINE TABLE knowledge_generation SCHEMAFULL CHANGEFEED 7d PERMISSIONS NONE;
DEFINE FIELD id ON knowledge_generation TYPE uuid;
DEFINE FIELD tenant ON knowledge_generation TYPE string READONLY;
DEFINE FIELD document ON knowledge_generation TYPE object FLEXIBLE  READONLY;
DEFINE FIELD revision ON knowledge_generation TYPE string READONLY;
DEFINE FIELD collections ON knowledge_generation TYPE array<object> FLEXIBLE
    READONLY;
DEFINE FIELD state ON knowledge_generation TYPE "building" | "active" | "retired";
DEFINE FIELD created_at ON knowledge_generation TYPE datetime
    DEFAULT time::now()
    READONLY;
DEFINE INDEX knowledge_generation_scope ON knowledge_generation FIELDS tenant, state;

DEFINE TABLE knowledge_active SCHEMAFULL CHANGEFEED 7d PERMISSIONS NONE;
DEFINE FIELD id ON knowledge_active TYPE string;
DEFINE FIELD generation ON knowledge_active TYPE record<knowledge_generation>;

DEFINE TABLE knowledge_member SCHEMAFULL CHANGEFEED 7d PERMISSIONS NONE;
DEFINE FIELD id ON knowledge_member TYPE array;
DEFINE FIELD tenant ON knowledge_member TYPE string READONLY;
DEFINE FIELD generation ON knowledge_member TYPE record<knowledge_generation>
    READONLY
    REFERENCE ON DELETE CASCADE;
DEFINE FIELD collection ON knowledge_member TYPE record<knowledge_collection>
    READONLY;
DEFINE FIELD uri ON knowledge_member TYPE string READONLY;
DEFINE FIELD epoch ON knowledge_member TYPE int ASSERT $value > 0;
DEFINE FIELD stale ON knowledge_member TYPE bool;
DEFINE FIELD deleted ON knowledge_member TYPE bool;
DEFINE FIELD observation ON knowledge_member TYPE option<object> FLEXIBLE;
DEFINE INDEX knowledge_member_collection ON knowledge_member FIELDS generation, collection, stale;

DEFINE TABLE knowledge_coverage SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON knowledge_coverage TYPE array;
DEFINE FIELD generation ON knowledge_coverage TYPE record<knowledge_generation>
    REFERENCE ON DELETE CASCADE;
DEFINE FIELD collection ON knowledge_coverage TYPE record<knowledge_collection>;
DEFINE FIELD revision ON knowledge_coverage TYPE string;

DEFINE ANALYZER knowledge_text TOKENIZERS blank, class FILTERS lowercase, ascii;

DEFINE TABLE knowledge_coordinator SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD id ON knowledge_coordinator TYPE string;
DEFINE FIELD owner ON knowledge_coordinator TYPE uuid;
DEFINE FIELD epoch ON knowledge_coordinator TYPE int ASSERT $value > 0;
DEFINE FIELD expires_at ON knowledge_coordinator TYPE datetime;

DEFINE TABLE knowledge_sync SCHEMAFULL CHANGEFEED 7d PERMISSIONS NONE;
DEFINE FIELD id ON knowledge_sync TYPE array;
DEFINE FIELD generation ON knowledge_sync TYPE record<knowledge_generation> REFERENCE ON DELETE CASCADE;
DEFINE FIELD collection ON knowledge_sync TYPE record<knowledge_collection>;
DEFINE FIELD coordinator ON knowledge_sync TYPE record<knowledge_coordinator>;
DEFINE FIELD owner_epoch ON knowledge_sync TYPE int ASSERT $value > 0;
DEFINE FIELD epoch ON knowledge_sync TYPE int ASSERT $value > 0;
DEFINE FIELD ready ON knowledge_sync TYPE bool;
DEFINE FIELD changed_at ON knowledge_sync TYPE datetime;
DEFINE INDEX knowledge_sync_generation ON knowledge_sync FIELDS generation, collection UNIQUE;

DEFINE FIELD sync ON knowledge_member TYPE record<knowledge_sync> READONLY;
DEFINE FIELD source_epoch ON knowledge_member TYPE int ASSERT $value > 0;
DEFINE FIELD valid_until ON knowledge_member TYPE option<datetime>;
DEFINE FIELD title ON knowledge_member TYPE option<string>;
