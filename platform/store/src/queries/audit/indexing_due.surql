SELECT * FROM audit_indexing_window WHERE end <= time::now() ORDER BY end LIMIT 32 TIMEOUT 10s;
