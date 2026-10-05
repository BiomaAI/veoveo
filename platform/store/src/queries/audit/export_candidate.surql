-- Scan partition heads, not the retained history of already exported blocks.
LET $candidate = (SELECT id, partition,
        math::max([
            type::record('audit_export_cursor', [$destination, partition]).sequence ?? 0,
            type::record('audit_retention_anchor', partition).sequence ?? 0
        ]) + 1 AS next_sequence,
        type::record('audit_export_cursor', [$destination, partition]).delivered_at ?? d'1970-01-01T00:00:00Z' AS last_delivery
    FROM audit_partition_head
    WHERE sequence > math::max([
        type::record('audit_export_cursor', [$destination, partition]).sequence ?? 0,
        type::record('audit_retention_anchor', partition).sequence ?? 0
    ])
    ORDER BY last_delivery ASC, id ASC LIMIT 1)[0];
RETURN IF $candidate = NONE {
    []
} ELSE {
    LET $block = (SELECT id, partition, sequence, head_hash, block FROM ONLY type::record('audit_block', [$candidate.partition, $candidate.next_sequence]));
    IF $block = NONE { THROW 'audit_block_conflict'; };
    [$block]
};
