BEGIN TRANSACTION;
LET $current = (SELECT * FROM ONLY $recording FOR UPDATE);
LET $source = (SELECT * FROM ONLY $dataset FOR UPDATE);
IF $current.tenant != $tenant OR $current.state != 'sealing' OR $current.revision != $revision OR $current.dataset != $dataset OR $source.tenant != $tenant OR $source.revision != $dataset_revision OR $content.recording_revision != $revision OR $content.dataset_revision != $dataset_revision OR $content.actor != $actor OR $content.source_owner != $current.owner OR $content.source_work_context != $current.work_context { THROW 'recording_manifest_publication_source_conflict'; };
LET $selected_layers = (SELECT id FROM recording_layer WHERE tenant = $tenant AND recording = $recording);
LET $expected_layers = $layers.map(|$layer| $layer.layer);
IF array::len($selected_layers) != array::len($layers) OR array::len(array::distinct($expected_layers)) != array::len($layers) { THROW 'recording_manifest_publication_layer_set_conflict'; };
FOR $layer IN $layers {
    LET $selected_layer_id = $layer.layer;
    LET $selected = (SELECT * FROM ONLY $selected_layer_id FOR UPDATE);
    IF $selected.tenant != $tenant OR $selected.recording != $recording OR $selected.state != 'committed' OR $selected.revision != $layer.revision { THROW 'recording_manifest_publication_layer_conflict'; };
};
LET $blueprints = (SELECT * FROM recording_blueprint WHERE tenant = $tenant AND recording = $recording ORDER BY revision DESC LIMIT 1);
IF $blueprint = NONE {
    IF array::len($blueprints) != 0 { THROW 'recording_manifest_publication_blueprint_conflict'; };
} ELSE {
    LET $selected_blueprint_id = $blueprint.id;
    LET $selected_blueprint = (SELECT * FROM ONLY $selected_blueprint_id FOR UPDATE);
    IF $selected_blueprint = NONE OR $blueprints[0].id != $blueprint.id OR $selected_blueprint.id != $blueprint.id OR $selected_blueprint.revision != $blueprint.revision OR $selected_blueprint.artifact != $blueprint.artifact { THROW 'recording_manifest_publication_blueprint_conflict'; };
};
CREATE ONLY $publication CONTENT $content RETURN NONE;
COMMIT TRANSACTION;
