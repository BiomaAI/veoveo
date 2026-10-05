BEGIN TRANSACTION;
            LET $expired = SELECT VALUE id FROM audit_download_window WHERE window_start < time::now() - 1d ORDER BY window_start ASC LIMIT 128;
            DELETE $expired; RETURN array::len($expired); COMMIT TRANSACTION;
