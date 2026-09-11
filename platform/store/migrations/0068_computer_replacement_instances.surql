-- Initial instances have no replacement ID. Existing rows and initial command
-- envelopes retain that canonical representation. Drain older Computer workers
-- before admitting replacements; they cannot address a replacement instance.
DEFINE FIELD replacement_instance_id ON computer TYPE option<uuid>
    ASSERT $value = NONE OR ($value != $this.computer_id AND $value != u'00000000-0000-0000-0000-000000000000');
DEFINE FIELD replacement_instance_id ON computer_operation TYPE option<uuid>
    ASSERT $value = NONE OR ($value != $this.computer_id AND $value != u'00000000-0000-0000-0000-000000000000');
