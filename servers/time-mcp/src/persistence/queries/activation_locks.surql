-- Lock both exact pointer IDs, even when one or both records are absent.
-- Callers bind pointer and release IDs in stable order before snapshot comparison.
FOR $pointer IN $pointers {
    LET $locked_pointer = (SELECT * FROM ONLY $pointer FOR UPDATE);
};
FOR $release_to_lock IN $releases {
    LET $locked_release = (SELECT * FROM ONLY $release_to_lock FOR UPDATE);
};
