BEGIN TRANSACTION;
            LET $prior = (SELECT * FROM ONLY $record);
            IF $prior = NONE { CREATE ONLY $record CONTENT $row RETURN NONE; }
            ELSE IF $prior != $row { THROW 'immutable_map_derivation_conflict'; };
            COMMIT TRANSACTION;
