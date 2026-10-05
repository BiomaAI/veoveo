BEGIN TRANSACTION;
LET $current = (SELECT * FROM ONLY $window);
IF $current != NONE AND $current.end <= time::now() AND $current.reads = $reads {
    fn::append_audit($audit_rows);
    DELETE ONLY $window;
};
COMMIT TRANSACTION;
