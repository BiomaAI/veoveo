SELECT VALUE producer_profile FROM type::table($chunk_table) ORDER BY ordinal;
SELECT VALUE id FROM knowledge_embedding_batch WHERE generation = $generation;
SELECT VALUE profile_epoch FROM ONLY $generation;
