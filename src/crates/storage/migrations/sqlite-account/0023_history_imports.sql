-- History imports of a data source with the import profile: a file is read in
-- a dry run, whose report is kept with the file until the import is committed
-- or expires. Committing writes the readings and daily summaries the dry run
-- accepted and keeps the result; the file itself is dropped then.
CREATE TABLE history_imports (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    data_source_id BLOB NOT NULL REFERENCES data_sources(id) ON DELETE CASCADE CHECK (length(data_source_id) = 16),
    requested_by BLOB CHECK (requested_by IS NULL OR length(requested_by) = 16),
    file_name TEXT NOT NULL CHECK (length(file_name) BETWEEN 1 AND 200),
    file_kind TEXT NOT NULL CHECK (file_kind IN ('status', 'output')),
    content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),
    content BLOB,
    state TEXT NOT NULL CHECK (state IN ('ready', 'committed')),
    report_json TEXT NOT NULL CHECK (json_valid(report_json)),
    result_json TEXT CHECK (result_json IS NULL OR json_valid(result_json)),
    created_at INTEGER NOT NULL,
    committed_at INTEGER,
    expires_at INTEGER NOT NULL
) STRICT;

CREATE INDEX history_imports_system_created_idx
    ON history_imports(system_id, created_at);
