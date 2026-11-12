-- Events a device reports, such as a fault or a changed setting. An event is
-- identified within its device by event_key: the source's own event
-- identifier, or its time and code when the source has none. Uploading an
-- event again therefore stores nothing new.
CREATE TABLE telemetry.device_events (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    -- The device as the source names it; for an inverter its serial.
    device_key TEXT NOT NULL CHECK (char_length(device_key) BETWEEN 1 AND 64),
    event_key TEXT NOT NULL CHECK (char_length(event_key) BETWEEN 1 AND 160),
    data_source_id UUID,
    occurred_at BIGINT NOT NULL,
    code BIGINT,
    message TEXT CHECK (message IS NULL OR char_length(message) <= 500),
    received_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, system_id, device_key, event_key),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);

CREATE INDEX device_events_time_idx
    ON telemetry.device_events(account_id, system_id, occurred_at DESC);
CREATE INDEX device_events_device_idx
    ON telemetry.device_events(account_id, system_id, device_key, occurred_at DESC);
