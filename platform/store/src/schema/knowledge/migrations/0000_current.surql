-- Current owner schema; schema definitions only.

DEFINE ANALYZER knowledge_text TOKENIZERS BLANK,CLASS FILTERS LOWERCASE, ASCII;

DEFINE TABLE knowledge_active TYPE NORMAL SCHEMAFULL CHANGEFEED 1w PERMISSIONS NONE;

DEFINE TABLE knowledge_collection TYPE NORMAL SCHEMAFULL CHANGEFEED 1w PERMISSIONS NONE;

DEFINE TABLE knowledge_coordinator TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE knowledge_coverage TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE knowledge_evaluation TYPE NORMAL SCHEMAFULL PERMISSIONS NONE;

DEFINE TABLE knowledge_generation TYPE NORMAL SCHEMAFULL CHANGEFEED 1w PERMISSIONS NONE;

DEFINE TABLE knowledge_member TYPE NORMAL SCHEMAFULL CHANGEFEED 1w PERMISSIONS NONE;

DEFINE TABLE knowledge_sync TYPE NORMAL SCHEMAFULL CHANGEFEED 1w PERMISSIONS NONE;

DEFINE FIELD OVERWRITE generation ON knowledge_active TYPE record<knowledge_generation> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON knowledge_active TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE approved ON knowledge_collection TYPE bool PERMISSIONS FULL;

DEFINE FIELD OVERWRITE collection ON knowledge_collection TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE document ON knowledge_collection TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE enumeration_root ON knowledge_collection TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON knowledge_collection TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON knowledge_collection TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON knowledge_collection TYPE string PERMISSIONS FULL;

DEFINE INDEX knowledge_collection_root ON knowledge_collection FIELDS tenant, enumeration_root;

DEFINE INDEX knowledge_collection_scope ON knowledge_collection FIELDS tenant, collection UNIQUE;

DEFINE FIELD OVERWRITE epoch ON knowledge_coordinator TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE expires_at ON knowledge_coordinator TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON knowledge_coordinator TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner ON knowledge_coordinator TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE collection ON knowledge_coverage TYPE record<knowledge_collection> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE generation ON knowledge_coverage TYPE record<knowledge_generation> REFERENCE ON DELETE CASCADE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON knowledge_coverage TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON knowledge_coverage TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE document ON knowledge_evaluation TYPE object FLEXIBLE READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE generation ON knowledge_evaluation TYPE record<knowledge_generation> READONLY REFERENCE ON DELETE CASCADE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON knowledge_evaluation TYPE string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE recorded_at ON knowledge_evaluation TYPE datetime DEFAULT time::now() READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON knowledge_evaluation TYPE string READONLY PERMISSIONS FULL;

DEFINE INDEX knowledge_evaluation_generation ON knowledge_evaluation FIELDS tenant, generation;

DEFINE FIELD OVERWRITE collections ON knowledge_generation TYPE array<object> FLEXIBLE READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE collections.* ON knowledge_generation TYPE object FLEXIBLE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE created_at ON knowledge_generation TYPE datetime DEFAULT time::now() READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE document ON knowledge_generation TYPE object FLEXIBLE READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON knowledge_generation TYPE uuid PERMISSIONS FULL;

DEFINE FIELD OVERWRITE revision ON knowledge_generation TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE state ON knowledge_generation TYPE 'building' | 'active' | 'retired' PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON knowledge_generation TYPE string READONLY PERMISSIONS FULL;

DEFINE INDEX knowledge_generation_scope ON knowledge_generation FIELDS tenant, state;

DEFINE FIELD OVERWRITE collection ON knowledge_member TYPE record<knowledge_collection> READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE deleted ON knowledge_member TYPE bool PERMISSIONS FULL;

DEFINE FIELD OVERWRITE epoch ON knowledge_member TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE generation ON knowledge_member TYPE record<knowledge_generation> READONLY REFERENCE ON DELETE CASCADE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON knowledge_member TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE observation ON knowledge_member TYPE none | { collection: string, revision: string, contentSha256: string, observedAt: string, modifiedAt: option<string>, modifiedBy: option<{ kind: 'principal', id: string }>, access: option<{ tenant: string, workContext: string, readPolicy: { kind: 'tenant' | 'subjects' | 'work-context' | 'selected-work-context' | 'selected-work-context-members' } | { kind: 'subjects-in-context', profile: option<string> }, owner: { kind: 'principal' | 'group', id: string }, grants: option<array<{ subject: { kind: 'principal' | 'group', id: string }, expiresAt: option<string> }>>, dataLabels: array<string>, expiresAt: option<string> }>, external: option<{ system: string, nativeId: string, url: option<string>, mirroredAt: option<string> }>, notModified: option<bool> } PERMISSIONS FULL;

DEFINE FIELD OVERWRITE source_epoch ON knowledge_member TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE stale ON knowledge_member TYPE bool PERMISSIONS FULL;

DEFINE FIELD OVERWRITE sync ON knowledge_member TYPE record<knowledge_sync> READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE tenant ON knowledge_member TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE title ON knowledge_member TYPE none | string PERMISSIONS FULL;

DEFINE FIELD OVERWRITE uri ON knowledge_member TYPE string READONLY PERMISSIONS FULL;

DEFINE FIELD OVERWRITE valid_until ON knowledge_member TYPE none | datetime PERMISSIONS FULL;

DEFINE INDEX knowledge_member_collection ON knowledge_member FIELDS generation, collection, stale;

DEFINE FIELD OVERWRITE changed_at ON knowledge_sync TYPE datetime PERMISSIONS FULL;

DEFINE FIELD OVERWRITE collection ON knowledge_sync TYPE record<knowledge_collection> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE coordinator ON knowledge_sync TYPE record<knowledge_coordinator> PERMISSIONS FULL;

DEFINE FIELD OVERWRITE epoch ON knowledge_sync TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE generation ON knowledge_sync TYPE record<knowledge_generation> REFERENCE ON DELETE CASCADE PERMISSIONS FULL;

DEFINE FIELD OVERWRITE id ON knowledge_sync TYPE array PERMISSIONS FULL;

DEFINE FIELD OVERWRITE owner_epoch ON knowledge_sync TYPE int ASSERT $value > 0 PERMISSIONS FULL;

DEFINE FIELD OVERWRITE ready ON knowledge_sync TYPE bool PERMISSIONS FULL;

DEFINE INDEX knowledge_sync_generation ON knowledge_sync FIELDS generation, collection UNIQUE;

DEFINE FIELD OVERWRITE approval ON knowledge_collection TYPE { collection: string, mode: "catalog-only" | "index", stewards: array<string>, authoritative_for: array<string>, data_labels: array<string> } PERMISSIONS FULL;
DEFINE FIELD OVERWRITE required_scopes ON knowledge_collection TYPE array<string> PERMISSIONS FULL;
DEFINE FIELD OVERWRITE change_signal ON knowledge_collection TYPE 'listen' | 'immutable' | 'revalidate' PERMISSIONS FULL;
DEFINE FIELD OVERWRITE entity_kind ON knowledge_collection TYPE string PERMISSIONS FULL;
