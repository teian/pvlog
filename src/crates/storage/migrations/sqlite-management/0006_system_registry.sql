-- Maps globally addressed systems to their owning account before account storage is opened.
CREATE TABLE system_registry (
    system_id BLOB PRIMARY KEY CHECK (length(system_id) = 16),
    account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(account_id) = 16),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;

CREATE INDEX system_registry_account_idx
    ON system_registry(account_id, system_id);
