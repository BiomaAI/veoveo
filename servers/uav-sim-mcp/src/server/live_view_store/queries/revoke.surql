BEGIN TRANSACTION;
LET $current = SELECT * FROM ONLY $key;
IF $current = NONE OR $current.generation != $generation OR $current.issuer != $issuer OR $current.closed_at != NONE {
    THROW 'uav_live_view_cas_conflict';
};
UPDATE ONLY $key CONTENT $grant RETURN NONE;
fn::append_audit($audit_rows);
COMMIT TRANSACTION;
