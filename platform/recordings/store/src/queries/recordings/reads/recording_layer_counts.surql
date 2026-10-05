BEGIN TRANSACTION;
             SELECT VALUE count() FROM recording_layer
             WHERE tenant = $tenant AND recording = $recording GROUP ALL;
             SELECT VALUE count() FROM recording_layer
             WHERE tenant = $tenant AND recording = $recording AND state = 'committed' GROUP ALL;
             COMMIT TRANSACTION;
