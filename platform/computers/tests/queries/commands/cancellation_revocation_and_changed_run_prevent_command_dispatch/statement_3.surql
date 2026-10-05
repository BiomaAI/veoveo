BEGIN; LET $saved = (SELECT * FROM computer_execution_payload)[0];
                    DELETE $saved.id;
                    CREATE $saved.id CONTENT { journal: $saved.journal,
                        sealed: object::extend($saved.sealed, {ciphertext: 'invalid-ciphertext'}) };
                    COMMIT;
