DEFINE EVENT fixture_seal_failure ON TABLE audit_block
            WHEN $event = 'CREATE' THEN { THROW 'fixture_seal_failure'; };
