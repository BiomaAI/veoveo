BEGIN TRANSACTION;
        LET $windows = SELECT * FROM audit_indexing_window;
        FOR $window IN $windows {
            DELETE ONLY $window.id;
            CREATE ONLY $window.id CONTENT object::extend($window, {
                start: $window.start - 5m, end: $window.end - 5m
            });
        }; COMMIT TRANSACTION;
