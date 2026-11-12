-- History imports of a data source with the import profile: a file is read in
-- a dry run, whose report is kept with the file until the import is committed
-- or expires. Committing writes the readings and daily summaries the dry run
-- accepted and keeps the result; the file itself is dropped then.
CREATE TABLE account_data.history_imports (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    data_source_id UUID NOT NULL,
    requested_by UUID,
    file_name TEXT NOT NULL CHECK (char_length(file_name) BETWEEN 1 AND 200),
    file_kind TEXT NOT NULL CHECK (file_kind IN ('status', 'output')),
    content_hash BYTEA NOT NULL CHECK (length(content_hash) = 32),
    content BYTEA,
    state TEXT NOT NULL CHECK (state IN ('ready', 'committed')),
    report JSONB NOT NULL,
    result JSONB,
    created_at BIGINT NOT NULL,
    committed_at BIGINT,
    expires_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, id),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, data_source_id)
        REFERENCES account_data.data_sources(account_id, id) ON DELETE CASCADE
);

CREATE INDEX history_imports_system_created_idx
    ON account_data.history_imports(account_id, system_id, created_at);
