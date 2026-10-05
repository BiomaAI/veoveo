SELECT VALUE id FROM ONLY $id
WHERE partition = $actor_partition
    AND actor_key = $actor
    AND draft.authority.profile = $profile
    AND draft.target = $target
    AND draft.detail = $detail
    AND outcome = 'allowed'
    AND occurred_at > time::now() - 15m;
