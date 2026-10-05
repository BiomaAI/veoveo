UPDATE ONLY $layer SET artifact = $layer_artifact; UPDATE ONLY $recording SET manifest_artifact = $manifest, state = 'sealed', ended_at = time::now(), sealed_at = time::now();
