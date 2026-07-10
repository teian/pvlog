CREATE TABLE pvlog_schema_identity (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    schema_kind TEXT NOT NULL CHECK (schema_kind = 'management')
);

INSERT INTO pvlog_schema_identity (singleton, schema_kind)
VALUES (1, 'management');
