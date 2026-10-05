FOR $n IN 0..1030 { CREATE type::record('audit_indexing_receipt', $n)
            SET fingerprint = 'fixture', recorded_at = time::now() - 2d; };
