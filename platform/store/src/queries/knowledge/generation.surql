BEGIN TRANSACTION;
LET $leader = (SELECT * FROM ONLY $coordinator WHERE owner = $owner AND epoch = $owner_epoch AND expires_at > time::now());
IF $leader = NONE { THROW 'knowledge_coordinator_fenced'; };
-- Write the row to serialize this transaction against takeover and expiry renewal.
UPDATE ONLY $coordinator SET expires_at = $leader.expires_at;

    -- Bound requirements are nominal native {record, revision} rows.
    FOR $required IN $collections {
        LET $catalog = (
            SELECT *
            FROM ONLY $required.record
            WHERE
                tenant = $tenant
                AND approved = true
                AND revision = $required.revision
        );
        IF $catalog = NONE {
            THROW 'knowledge_collection_not_approved';
        };
    };
    CREATE ONLY $generation CONTENT {
        tenant: $tenant,
        document: $document,
        revision: $revision,
        collections: $collections,
        state: 'building',
    };
    FOR $required IN $collections {
        LET $sync = type::record('knowledge_sync', [<string>record::id($generation), $required.record.collection]);
        CREATE ONLY $sync CONTENT {generation: $generation, collection: $required.record,
            coordinator: $coordinator, owner_epoch: $owner_epoch, epoch: 1, ready: false, changed_at: time::now()};
    };
    DEFINE TABLE $chunk_table SCHEMAFULL PERMISSIONS NONE;
    DEFINE FIELD member ON $chunk_table TYPE record<knowledge_member>
        REFERENCE ON DELETE CASCADE;
    DEFINE FIELD tenant ON $chunk_table TYPE string;
    DEFINE FIELD collection ON $chunk_table TYPE record<knowledge_collection>;
    DEFINE FIELD collection_id ON $chunk_table TYPE string;
    DEFINE FIELD approval_revision ON $chunk_table TYPE string;
    DEFINE FIELD uri ON $chunk_table TYPE string;
    DEFINE FIELD ordinal ON $chunk_table TYPE int
        ASSERT $value >= 0
        AND $value < 256;
    DEFINE FIELD admission ON $chunk_table TYPE object;
    DEFINE FIELD admission.tenant_read ON $chunk_table TYPE bool;
    DEFINE FIELD admission.context_read ON $chunk_table TYPE bool;
    DEFINE FIELD admission.selected_context_read ON $chunk_table TYPE bool;
    DEFINE FIELD admission.expires_at ON $chunk_table TYPE option<datetime>;
    DEFINE FIELD admission.work_context ON $chunk_table TYPE option<string>;
    DEFINE FIELD admission.required_context ON $chunk_table TYPE option<string>;
    DEFINE FIELD admission.required_profile ON $chunk_table TYPE option<string>;
    DEFINE FIELD admission.grants ON $chunk_table TYPE array<object>;
    DEFINE FIELD admission.grants.*.subject ON $chunk_table TYPE string;
    DEFINE FIELD admission.grants.*.expires_at ON $chunk_table TYPE option<datetime>;
    DEFINE FIELD admission.labels ON $chunk_table TYPE array<string>;
    DEFINE FIELD text ON $chunk_table TYPE string
        ASSERT string::len($value) > 0
        AND string::len($value) <= 32768;
    DEFINE FIELD title ON $chunk_table TYPE string;
    DEFINE FIELD embedding ON $chunk_table TYPE array<float, __DIMENSION__>;
    DEFINE FIELD observation ON $chunk_table TYPE { collection: string, revision: string, contentSha256: string, observedAt: string, modifiedAt: option<string>, modifiedBy: option<{ kind: 'principal', id: string }>, access: option<{ tenant: string, workContext: string, readPolicy: { kind: 'tenant' | 'subjects' | 'work-context' | 'selected-work-context' | 'selected-work-context-members' } | { kind: 'subjects-in-context', profile: option<string> }, owner: { kind: 'principal' | 'group', id: string }, grants: option<array<{ subject: { kind: 'principal' | 'group', id: string }, expiresAt: option<string> }>>, dataLabels: array<string>, expiresAt: option<string> }>, external: option<{ system: string, nativeId: string, url: option<string>, mirroredAt: option<string> }>, notModified: option<bool> };
    DEFINE INDEX member_ordinal ON $chunk_table FIELDS member, ordinal UNIQUE;
    DEFINE INDEX content_search ON $chunk_table FIELDS text FULLTEXT ANALYZER knowledge_text BM25;
    DEFINE INDEX embedding_search ON $chunk_table FIELDS embedding
        HNSW DIMENSION __DIMENSION__ DIST COSINE;
COMMIT TRANSACTION;
