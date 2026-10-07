BEGIN TRANSACTION;
LET $parent_id = $content.recording;
LET $parent = (SELECT * FROM ONLY $parent_id FOR UPDATE);
LET $intent = (SELECT * FROM ONLY $publication FOR UPDATE);
IF $parent = NONE OR $parent.tenant != $content.tenant { THROW 'recording_layer_parent_conflict'; };
IF $intent != NONE AND NOT ($parent.state = 'sealed' AND $content.kind = 'derived' AND type::string(record::id($layer)) NOT IN $intent.body.layers.layerId) { THROW 'recording_source_selection_frozen'; };
IF ($content.kind = 'capture' AND $parent.state != 'live') OR ($content.kind = 'properties' AND $parent.state != 'sealing') OR ($content.kind = 'derived' AND $parent.state NOT IN ['ready', 'sealed', 'interrupted']) { THROW 'recording_layer_parent_conflict'; };
IF $content.kind = 'properties' {
    LET $sources = (SELECT VALUE id FROM recording_layer WHERE tenant = $content.tenant AND recording = $content.recording AND kind != 'properties');
    IF array::len($sources) != array::len($source_layers) OR array::len(array::distinct($source_layers.layer)) != array::len($source_layers) { THROW 'recording_properties_preparation_source_conflict'; };
    FOR $selection IN $source_layers {
        LET $source_id = $selection.layer;
        LET $source = (SELECT * FROM ONLY $source_id FOR UPDATE);
        IF $source = NONE OR $source.tenant != $content.tenant OR $source.recording != $content.recording OR $source.kind = 'properties' OR $source.state != 'committed' OR $source.revision != $selection.revision OR $source.id NOT IN $sources { THROW 'recording_properties_preparation_source_conflict'; };
    };
    LET $dataset_id = $parent.dataset;
    LET $dataset = (SELECT * FROM ONLY $dataset_id FOR UPDATE);
    LET $body = $content.properties_preparation.body;
    IF $content.properties_preparation = NONE OR $content.properties_preparation.version != 1 OR $body.recording_id != type::string(record::id($parent.id)) OR $body.dataset_id != type::string(record::id($parent.dataset)) OR $body.source_revision != $parent.revision OR <datetime>$body.sealed_at != $parent.updated_at OR $body.dataset_key != $dataset.dataset_key OR $body.producer_recording_key != $parent.recording_key OR <datetime>$body.started_at != $parent.started_at OR <datetime>$body.ended_at != $parent.ended_at { THROW 'recording_properties_preparation_source_conflict'; };
};
CREATE ONLY $layer CONTENT $content RETURN NONE;
UPDATE ONLY $content.recording SET revision += 1, updated_at = time::now() RETURN NONE;
COMMIT TRANSACTION;
