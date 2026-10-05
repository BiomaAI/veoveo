DEFINE TABLE agent SCHEMAFULL CHANGEFEED 1h INCLUDE ORIGINAL;
         DEFINE FIELD lease ON agent TYPE datetime;
         DEFINE TABLE changefeed_noise_fixture SCHEMAFULL CHANGEFEED 1h INCLUDE ORIGINAL;
         DEFINE FIELD ordinal ON changefeed_noise_fixture TYPE int;
