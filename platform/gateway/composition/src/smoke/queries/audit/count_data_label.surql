SELECT VALUE count()
FROM audit_record:[$partition, u"00000000-0000-0000-0000-000000000000"]..=[$partition, u"ffffffff-ffff-ffff-ffff-ffffffffffff"]
WHERE partition = $partition
    AND ($outcome = NONE OR outcome = $outcome)
    AND (class = $selection.value.class AND $selection.value.label IN draft.authority.data_labels)
GROUP ALL;
