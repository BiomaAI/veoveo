BEGIN TRANSACTION;
            CREATE audit_domain_fixture:one SET revision = 1;
            fn::append_audit($audit_rows);
            THROW 'injected_domain_failure'; COMMIT TRANSACTION;
