CREATE TABLE projection_outbox_state (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    current_sequence INTEGER NOT NULL DEFAULT 0 CHECK (current_sequence >= 0)
) STRICT;

INSERT INTO projection_outbox_state (singleton, current_sequence) VALUES (1, 0);

CREATE TABLE projection_outbox_events (
    source_sequence INTEGER PRIMARY KEY CHECK (source_sequence > 0),
    event_id BLOB NOT NULL UNIQUE CHECK (length(event_id) = 16),
    event_kind TEXT NOT NULL CHECK (event_kind IN ('upsert', 'invalidate', 'delete')),
    system_id BLOB NOT NULL CHECK (length(system_id) = 16),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    privacy_reducing INTEGER NOT NULL CHECK (privacy_reducing IN (0, 1)),
    invalidation_id BLOB CHECK (invalidation_id IS NULL OR length(invalidation_id) = 16),
    created_at INTEGER NOT NULL
) STRICT;

CREATE INDEX projection_outbox_events_created_idx
    ON projection_outbox_events(created_at, source_sequence);

CREATE TRIGGER projection_outbox_events_no_update
BEFORE UPDATE ON projection_outbox_events
BEGIN
    SELECT RAISE(ABORT, 'projection outbox events are append-only');
END;

CREATE TRIGGER projection_outbox_events_no_delete
BEFORE DELETE ON projection_outbox_events
BEGIN
    SELECT RAISE(ABORT, 'projection outbox events are append-only');
END;

CREATE TABLE projection_outbox_deliveries (
    source_sequence INTEGER PRIMARY KEY REFERENCES projection_outbox_events(source_sequence) ON DELETE RESTRICT,
    delivery_attempts INTEGER NOT NULL DEFAULT 0 CHECK (delivery_attempts >= 0),
    last_attempt_at INTEGER NOT NULL,
    delivered_at INTEGER,
    last_error_code TEXT
) STRICT;

CREATE INDEX projection_outbox_pending_idx
    ON projection_outbox_deliveries(delivered_at, last_attempt_at, source_sequence)
    WHERE delivered_at IS NULL;
