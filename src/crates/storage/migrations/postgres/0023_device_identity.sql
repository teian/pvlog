-- What a source last reported about each of its devices, whether or not the
-- device is known as an inverter. A member the source leaves out keeps the
-- value reported before.
CREATE TABLE account_data.reported_devices (
    account_id UUID NOT NULL,
    system_id UUID NOT NULL,
    -- The device as the source names it; for an inverter its serial.
    device_serial TEXT NOT NULL CHECK (char_length(device_serial) BETWEEN 1 AND 64),
    kind TEXT NOT NULL DEFAULT 'inverter',
    family_id TEXT,
    configured_name TEXT,
    reported_name TEXT,
    model TEXT,
    firmware TEXT,
    transport TEXT,
    first_reported_at BIGINT NOT NULL,
    last_reported_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, system_id, device_serial),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);

-- Firmware is reported by the device, never entered. The time records when a
-- different version was first reported for the inverter.
ALTER TABLE account_data.inverters ADD COLUMN firmware_version TEXT;
ALTER TABLE account_data.inverters ADD COLUMN firmware_changed_at BIGINT;
