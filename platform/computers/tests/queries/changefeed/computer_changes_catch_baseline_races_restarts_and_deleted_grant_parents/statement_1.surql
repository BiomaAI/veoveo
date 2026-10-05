BEGIN TRANSACTION;
            CREATE ONLY $task SET content = 'never decoded by the identity reader';
            CREATE ONLY $computer_grant SET computer_id = $computer, grant_id = $grant;
            COMMIT TRANSACTION;
