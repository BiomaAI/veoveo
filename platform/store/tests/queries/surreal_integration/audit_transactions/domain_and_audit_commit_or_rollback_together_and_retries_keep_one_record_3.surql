BEGIN TRANSACTION;
            CREATE audit_domain_fixture:one SET revision = 1;
            fn::append_audit($audit_rows); COMMIT TRANSACTION;
