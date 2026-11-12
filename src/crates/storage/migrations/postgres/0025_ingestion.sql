-- Energy samples a device kept in its own archive: its lifetime counter and
-- power at a time, whether or not a live measurement exists for that time.
-- A sample is identified by its device and time, so sending it again stores
-- nothing new.
CREATE TABLE telemetry.device_energy_samples (
    account_id UUID NOT NULL,
    system_id UUID NOT NULL,
    -- The device as the source names it; for an inverter its serial.
    device_key TEXT NOT NULL CHECK (char_length(device_key) BETWEEN 1 AND 64),
    measured_at BIGINT NOT NULL,
    energy_lifetime_wh BIGINT CHECK (energy_lifetime_wh IS NULL OR energy_lifetime_wh >= 0),
    power_watts BIGINT,
    data_source_id UUID,
    received_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, system_id, device_key, measured_at),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);

-- How far a source has delivered: the latest measurement, sample, daily
-- summary and event PVLog holds from it per device. A logger continues from
-- these marks instead of sending everything again. device_key is empty for
-- values of the whole system.
CREATE TABLE telemetry.ingestion_marks (
    account_id UUID NOT NULL,
    system_id UUID NOT NULL,
    source_key TEXT NOT NULL,
    device_key TEXT NOT NULL DEFAULT '',
    measured_at BIGINT,
    sample_at BIGINT,
    summary_date TEXT,
    event_at BIGINT,
    PRIMARY KEY (account_id, system_id, source_key, device_key),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);
