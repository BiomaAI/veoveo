BEGIN TRANSACTION;
         CREATE agent:pilot SET lease = time::now() + 30s;
         CREATE changefeed_noise_fixture:coupled SET ordinal = 99;
         COMMIT TRANSACTION;
