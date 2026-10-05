BEGIN TRANSACTION;
            CREATE audit_domain_fixture:two SET revision = 2;
            fn::append_audit($audit_rows); COMMIT TRANSACTION;
