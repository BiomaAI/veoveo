BEGIN TRANSACTION;
LET $world = (SELECT * FROM frame_world WHERE tenant = $tenant AND owner = $owner
    AND world_key = $world_key AND labels ALLINSIDE $clearance LIMIT 1)[0];
IF $world = NONE { THROW 'frame world is unavailable for publication'; };
LET $head = (SELECT * FROM frame_world_revision WHERE id = $world.head_revision
    AND tenant = $tenant AND owner = $owner AND world = $world.id AND world_key = $world_key
    AND revision_key = $world.head_revision_key AND revision = $world.revision LIMIT 1)[0];
IF ($world.revision = 0 AND ($world.head_revision != NONE OR $world.head_revision_key != NONE))
    OR ($world.revision > 0 AND $head = NONE) {
    THROW 'frame world head metadata is inconsistent';
};
IF $head != NONE AND $head.spec_sha256 = $digest {
    RETURN { world: $world, revision: $head, created: false };
};
IF $world.head_revision_key != $expected_head { THROW 'frame world head conflicts'; };
LET $revision = CREATE ONLY $revision_record CONTENT {
    tenant: $tenant, owner: $owner, world: $world.id, world_key: $world_key,
    revision_key: $revision_key, revision: $world.revision + 1,
    spec_sha256: $digest, root_frame_key: $root, definition: $tree, frame_ids: $frame_ids, created_at: $now
};
LET $published = UPDATE ONLY $world.id SET head_revision = $revision_record,
    head_revision_key = $revision_key, revision = $world.revision + 1, updated_at = $now;
RETURN { world: $published, revision: $revision, created: true };
COMMIT TRANSACTION;
