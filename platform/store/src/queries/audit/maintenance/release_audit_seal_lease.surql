UPDATE ONLY audit_sealer:active SET lease_until = time::now()
            WHERE owner = $owner AND generation = $generation;
