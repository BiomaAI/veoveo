BEGIN TRANSACTION;
DELETE ONLY $revision;
CREATE ONLY $revision CONTENT object::extend($content, {
    content: object::extend($content.content, { model: {} })
}) RETURN NONE;
UPDATE ONLY $definition SET audience = [] RETURN NONE;
COMMIT TRANSACTION;
