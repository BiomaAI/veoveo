BEGIN TRANSACTION;
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
    DEFINE TABLE __TABLE__ SCHEMAFULL PERMISSIONS NONE;
    DEFINE FIELD member ON __TABLE__ TYPE record<knowledge_member>
        REFERENCE ON DELETE CASCADE;
    DEFINE FIELD tenant ON __TABLE__ TYPE string;
    DEFINE FIELD collection ON __TABLE__ TYPE record<knowledge_collection>;
    DEFINE FIELD collection_id ON __TABLE__ TYPE string;
    DEFINE FIELD approval_revision ON __TABLE__ TYPE string;
    DEFINE FIELD uri ON __TABLE__ TYPE string;
    DEFINE FIELD ordinal ON __TABLE__ TYPE int
        ASSERT $value >= 0
        AND $value < 256;
    DEFINE FIELD admission ON __TABLE__ TYPE object;
    DEFINE FIELD admission.tenant_read ON __TABLE__ TYPE bool;
    DEFINE FIELD admission.context_read ON __TABLE__ TYPE bool;
    DEFINE FIELD admission.selected_context_read ON __TABLE__ TYPE bool;
    DEFINE FIELD admission.expires_at ON __TABLE__ TYPE option<datetime>;
    DEFINE FIELD admission.work_context ON __TABLE__ TYPE option<string>;
    DEFINE FIELD admission.required_context ON __TABLE__ TYPE option<string>;
    DEFINE FIELD admission.required_profile ON __TABLE__ TYPE option<string>;
    DEFINE FIELD admission.grants ON __TABLE__ TYPE array<object>;
    DEFINE FIELD admission.grants.*.subject ON __TABLE__ TYPE string;
    DEFINE FIELD admission.grants.*.expires_at ON __TABLE__ TYPE option<datetime>;
    DEFINE FIELD admission.labels ON __TABLE__ TYPE array<string>;
    DEFINE FIELD text ON __TABLE__ TYPE string
        ASSERT string::len($value) > 0
        AND string::len($value) <= 32768;
    DEFINE FIELD embedding ON __TABLE__ TYPE array<float, __DIMENSION__>;
    DEFINE FIELD observation ON __TABLE__ TYPE object FLEXIBLE;
    DEFINE INDEX member_ordinal ON __TABLE__ FIELDS member, ordinal UNIQUE;
    DEFINE INDEX content_search ON __TABLE__ FIELDS text FULLTEXT ANALYZER knowledge_text BM25;
    DEFINE INDEX embedding_search ON __TABLE__ FIELDS embedding
        HNSW DIMENSION __DIMENSION__ DIST COSINE;
COMMIT TRANSACTION;
