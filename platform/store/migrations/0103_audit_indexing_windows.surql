-- Mutable staging is separate from the immutable, sealed audit log.
DEFINE TABLE audit_indexing_window SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD start ON audit_indexing_window TYPE datetime READONLY;
DEFINE FIELD end ON audit_indexing_window TYPE datetime READONLY;
DEFINE FIELD collection ON audit_indexing_window TYPE string READONLY;
DEFINE FIELD template ON audit_indexing_window TYPE object FLEXIBLE READONLY;
DEFINE FIELD reads ON audit_indexing_window TYPE int ASSERT $value > 0 AND $value <= 9007199254740991;
DEFINE FIELD not_modified ON audit_indexing_window TYPE int ASSERT $value >= 0;
DEFINE FIELD failed ON audit_indexing_window TYPE int ASSERT $value >= 0;
DEFINE FIELD members_digest ON audit_indexing_window TYPE string;
DEFINE INDEX audit_indexing_due ON audit_indexing_window FIELDS end;

-- Retry receipts contain only an identity and fingerprint, never member content.
DEFINE TABLE audit_indexing_receipt SCHEMAFULL PERMISSIONS NONE;
DEFINE FIELD fingerprint ON audit_indexing_receipt TYPE string READONLY;
DEFINE FIELD recorded_at ON audit_indexing_receipt TYPE datetime READONLY;
DEFINE INDEX audit_indexing_receipt_expiry ON audit_indexing_receipt FIELDS recorded_at;

DEFINE FUNCTION fn::append_audit_indexing($reads: array<object>) {
    IF array::len($reads) > 64 { THROW 'audit_batch_limit'; };
    LET $now = time::now();
    FOR $read IN $reads {
        LET $existing = (SELECT * FROM ONLY $read.receipt);
        IF $existing != NONE {
            IF $existing.fingerprint != $read.fingerprint { THROW 'audit_identity_conflict'; };
        } ELSE {
            IF $read.occurred_at < $now - 1h OR $read.occurred_at > $now + 1m {
                THROW 'audit_indexing_read_expired';
            };
            LET $start = time::floor($now, 5m);
            LET $id = type::record('audit_indexing_window', [$read.scope, $start]);
            LET $window = (SELECT * FROM ONLY $id);
            LET $previous = IF $window = NONE { $read.genesis } ELSE { $window.members_digest };
            LET $digest = IF $read.member = NONE { $previous } ELSE {
                crypto::sha256('veoveo.ai/audit-indexing-members/v1:' + $previous + $read.member)
            };
            IF $window = NONE {
                CREATE ONLY $id CONTENT {
                    start: $start, end: $start + 5m, collection: $read.collection,
                    template: $read.template, reads: 1, not_modified: $read.not_modified,
                    failed: $read.failed, members_digest: $digest
                } RETURN NONE;
            } ELSE {
                UPDATE ONLY $id SET reads += 1, not_modified += $read.not_modified,
                    failed += $read.failed, members_digest = $digest RETURN NONE;
            };
            CREATE ONLY $read.receipt CONTENT {
                fingerprint: $read.fingerprint, recorded_at: $now
            } RETURN NONE;
        };
    };
    RETURN NONE;
} PERMISSIONS NONE;
