-- Maps globally addressed systems to their owning account before account storage is opened.
CREATE TABLE management.system_registry (
    system_id UUID PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE INDEX system_registry_account_idx
    ON management.system_registry(account_id, system_id);
