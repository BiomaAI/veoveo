BEGIN TRANSACTION;
LET $expired = SELECT VALUE id FROM audit_indexing_receipt
    WHERE recorded_at < time::now() - 1d ORDER BY recorded_at LIMIT 1024;
DELETE $expired;
RETURN array::len($expired);
COMMIT TRANSACTION;
