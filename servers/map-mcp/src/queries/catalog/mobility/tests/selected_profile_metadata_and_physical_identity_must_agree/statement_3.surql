BEGIN; LET $content = (SELECT * OMIT id FROM ONLY $id); DELETE $id;
            CREATE $wrong CONTENT $content; UPDATE ONLY $wrong SET family = 'human' RETURN NONE; COMMIT;
