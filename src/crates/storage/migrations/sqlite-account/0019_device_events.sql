-- Events a device reports, such as a fault or a changed setting. An event is
-- identified within its device by event_key: the source's own event
-- identifier, or its time and code when the source has none. Uploading an
-- event again therefore stores nothing new.
CREATE TABLE device_events (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    -- The device as the source names it; for an inverter its serial.
    device_key TEXT NOT NULL CHECK (length(device_key) BETWEEN 1 AND 64),
    event_key TEXT NOT NULL CHECK (length(event_key) BETWEEN 1 AND 160),
    data_source_id BLOB CHECK (data_source_id IS NULL OR length(data_source_id) = 16),
    occurred_at INTEGER NOT NULL,
    code INTEGER,
    message TEXT CHECK (message IS NULL OR length(message) <= 500),
    received_at INTEGER NOT NULL,
    UNIQUE (system_id, device_key, event_key)
) STRICT;

CREATE INDEX device_events_time_idx
    ON device_events(system_id, occurred_at DESC);
CREATE INDEX device_events_device_idx
    ON device_events(system_id, device_key, occurred_at DESC);
