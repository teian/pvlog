-- What a source last reported about each of its devices, whether or not the
-- device is known as an inverter. A member the source leaves out keeps the
-- value reported before.
CREATE TABLE reported_devices (
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    -- The device as the source names it; for an inverter its serial.
    device_serial TEXT NOT NULL CHECK (length(device_serial) BETWEEN 1 AND 64),
    kind TEXT NOT NULL DEFAULT 'inverter',
    family_id TEXT,
    configured_name TEXT,
    reported_name TEXT,
    model TEXT,
    firmware TEXT,
    transport TEXT,
    first_reported_at INTEGER NOT NULL,
    last_reported_at INTEGER NOT NULL,
    PRIMARY KEY (system_id, device_serial)
) STRICT;

-- Firmware is reported by the device, never entered. The time records when a
-- different version was first reported for the inverter.
ALTER TABLE inverters ADD COLUMN firmware_version TEXT;
ALTER TABLE inverters ADD COLUMN firmware_changed_at INTEGER;
