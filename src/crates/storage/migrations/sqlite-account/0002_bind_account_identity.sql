CREATE TABLE pvlog_account_identity (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    account_id BLOB NOT NULL CHECK (length(account_id) = 16),
    bound_at INTEGER NOT NULL,
    UNIQUE (account_id)
) STRICT;
