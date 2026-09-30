SELECT id, partition, sequence, block FROM audit_block
WHERE sealed_at < $cutoff AND ($destinations ALLINSIDE exported_to)
    AND sequence = ((type::record('audit_retention_anchor', partition).sequence ?? 0) + 1)
ORDER BY id ASC LIMIT 64;
