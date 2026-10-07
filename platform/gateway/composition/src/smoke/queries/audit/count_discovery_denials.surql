SELECT VALUE count()
FROM audit_record:[$partition, u"00000000-0000-0000-0000-000000000000"]..=[$partition, u"ffffffff-ffff-ffff-ffff-ffffffffffff"]
WHERE partition = $partition
    AND ($outcome = NONE OR outcome = $outcome)
    AND (draft.detail.kind = 'discovery' AND draft.detail.collection = $selection.value.collection AND draft.detail.denied = $selection.value.denied)
GROUP ALL;
