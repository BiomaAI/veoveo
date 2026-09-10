LET $head = SELECT * FROM ONLY gateway_control_active:current;
LET $revision = IF $head = NONE { NONE } ELSE { (SELECT * FROM ONLY $head.revision) };
RETURN {head: $head, revision: $revision};
