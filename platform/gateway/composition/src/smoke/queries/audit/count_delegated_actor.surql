SELECT VALUE count()
FROM audit_record:[$partition, u"00000000-0000-0000-0000-000000000000"]..=[$partition, u"ffffffff-ffff-ffff-ffff-ffffffffffff"]
WHERE partition = $partition
    AND ($outcome = NONE OR outcome = $outcome)
    AND (class = $selection.value.class AND draft.actor.kind = 'service' AND draft.actor.principal = $selection.value.actor AND draft.actor.delegating_principal = $selection.value.delegator AND draft.actor.oauth_client = $selection.value.client)
GROUP ALL;
