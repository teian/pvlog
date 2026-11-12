-- Energy samples a device kept in its own archive: its lifetime counter and
-- power at a time, whether or not a live measurement exists for that time.
-- A sample is identified by its device and time, so sending it again stores
-- nothing new.
CREATE TABLE device_energy_samples (
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    -- The device as the source names it; for an inverter its serial.
    device_key TEXT NOT NULL CHECK (length(device_key) BETWEEN 1 AND 64),
    measured_at INTEGER NOT NULL,
    energy_lifetime_wh INTEGER CHECK (energy_lifetime_wh IS NULL OR energy_lifetime_wh >= 0),
    power_watts INTEGER,
    data_source_id BLOB CHECK (data_source_id IS NULL OR length(data_source_id) = 16),
    received_at INTEGER NOT NULL,
    PRIMARY KEY (system_id, device_key, measured_at)
) STRICT;

-- How far a source has delivered: the latest measurement, sample, daily
-- summary and event PVLog holds from it per device. A logger continues from
-- these marks instead of sending everything again. device_key is empty for
-- values of the whole system.
CREATE TABLE ingestion_marks (
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    source_key TEXT NOT NULL,
    device_key TEXT NOT NULL DEFAULT '',
    measured_at INTEGER,
    sample_at INTEGER,
    summary_date TEXT,
    event_at INTEGER,
    PRIMARY KEY (system_id, source_key, device_key)
) STRICT;
