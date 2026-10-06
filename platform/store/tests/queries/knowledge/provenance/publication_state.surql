SELECT VALUE id FROM knowledge_embedding_batch WHERE generation = $generation ORDER BY id;
SELECT VALUE producer_profiles FROM ONLY $generation;
SELECT VALUE profile_epoch FROM ONLY $generation;
