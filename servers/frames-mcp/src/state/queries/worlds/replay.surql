BEGIN TRANSACTION;
LET $world = (SELECT * FROM frame_world WHERE tenant = $tenant AND owner = $owner
    AND world_key = $world_key AND labels ALLINSIDE $clearance LIMIT 1)[0];
IF $world = NONE { RETURN NONE; };
LET $head = (SELECT * FROM frame_world_revision WHERE id = $world.head_revision
    AND tenant = $tenant AND owner = $owner AND world = $world.id AND world_key = $world_key
    AND revision_key = $world.head_revision_key AND revision = $world.revision
    AND spec_sha256 = $digest LIMIT 1)[0];
IF $head = NONE { RETURN NONE; };
RETURN { world: $world, revision: $head, created: false };
COMMIT TRANSACTION;
