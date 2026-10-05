UPDATE ONLY $coordinator SET expires_at = time::now() WHERE owner = $owner AND epoch = $owner_epoch;
