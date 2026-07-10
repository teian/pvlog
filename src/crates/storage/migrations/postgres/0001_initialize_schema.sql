CREATE TABLE pvlog_schema_identity (
    singleton BOOLEAN PRIMARY KEY DEFAULT TRUE CHECK (singleton),
    schema_kind TEXT NOT NULL CHECK (schema_kind = 'postgres')
);

INSERT INTO pvlog_schema_identity (singleton, schema_kind)
VALUES (TRUE, 'postgres');
