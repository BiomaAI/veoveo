BEGIN TRANSACTION;
LET $head = (SELECT * FROM ONLY $head_id FOR UPDATE);
LET $anchor = (SELECT * FROM ONLY $anchor_id FOR UPDATE);
RETURN { checkpoint: $head.checkpoint, retired: $anchor.sequence ?? 0 };
COMMIT TRANSACTION;
