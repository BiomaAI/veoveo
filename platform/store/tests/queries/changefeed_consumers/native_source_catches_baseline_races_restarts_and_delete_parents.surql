BEGIN TRANSACTION;
            CREATE ONLY $task SET content = 'never decoded by the identity reader';
            CREATE ONLY $artifact_grant SET in = $artifact;
            COMMIT TRANSACTION;
