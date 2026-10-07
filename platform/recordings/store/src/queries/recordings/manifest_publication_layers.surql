SELECT * FROM recording_layer
WHERE tenant = $tenant AND recording = $recording AND id IN $layers
AND array::len((SELECT VALUE id FROM recording_layer
    WHERE tenant = $tenant AND recording = $recording AND id NOT IN $layers
    AND (kind != 'derived' OR (SELECT VALUE state FROM ONLY $recording WHERE tenant = $tenant) != 'sealed'))) = 0;
