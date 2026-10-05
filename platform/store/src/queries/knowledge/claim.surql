BEGIN TRANSACTION;
LET $prior = (SELECT * FROM ONLY $coordinator);
IF $prior != NONE AND $prior.expires_at > time::now() AND $prior.owner != $owner {
    RETURN NONE;
} ELSE {
    LET $epoch = IF $prior != NONE AND $prior.owner = $owner AND $prior.expires_at > time::now() { $prior.epoch } ELSE { ($prior.epoch ?? 0) + 1 };
    UPSERT $coordinator SET owner = $owner, epoch = $epoch, expires_at = time::now() + 30s;
    RETURN $epoch;
};
COMMIT TRANSACTION;
