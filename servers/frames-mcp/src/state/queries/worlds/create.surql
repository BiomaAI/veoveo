BEGIN TRANSACTION;
LET $existing = (SELECT * FROM frame_world WHERE tenant = $tenant AND world_key = $world_key
    AND labels ALLINSIDE $clearance LIMIT 1)[0];
IF $existing != NONE {
    IF $existing.display_name != $content.display_name OR $existing.description != $content.description {
        THROW 'frame world metadata conflicts';
    };
    RETURN $existing;
};
LET $world = CREATE ONLY $world_record CONTENT $content;
RETURN $world;
COMMIT TRANSACTION;
