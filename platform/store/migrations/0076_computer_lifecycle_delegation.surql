-- Existing lifecycle operations were accepted only by their retained owner.
-- Drain old Computers readers/workers before admitting named lifecycle grants.
DEFINE FIELD owner_context ON computer_operation TYPE option<object> FLEXIBLE;
UPDATE computer_operation SET owner_context = actor_context WHERE owner_context = NONE;
DEFINE FIELD OVERWRITE owner_context ON computer_operation TYPE object FLEXIBLE;
DEFINE FIELD automation_grant_id ON computer_operation TYPE option<uuid>
  ASSERT $value = NONE OR ($value != u'00000000-0000-0000-0000-000000000000' AND $this.action != 'create');
