CREATE upgrade_guard:one SET revision = 1;
             UPDATE upgrade_guard:one SET revision = 2 WHERE revision = 1;
             UPSERT upgrade_guard:one SET revision = 3 WHERE revision = 2;
             UPDATE upgrade_guard:one SET revision = 4 WHERE revision = 4;
             UPSERT upgrade_guard:one SET revision = 5 WHERE revision = 5;
             SELECT VALUE revision FROM ONLY upgrade_guard:one;
