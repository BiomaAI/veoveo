BEGIN TRANSACTION;
LET $preparation = (SELECT * FROM ONLY platform_module_installation:current FOR UPDATE);
IF $preparation.generation != $generation OR $preparation.identity != $identity OR $preparation.complete != true { THROW 'installation_preparation_changed'; };
LET $head = (SELECT * FROM ONLY gateway_control_active:current FOR UPDATE);
LET $revision = IF $head = NONE { NONE } ELSE { (SELECT * FROM ONLY $head.revision) };
RETURN { head: $head, revision: $revision };
COMMIT TRANSACTION;
