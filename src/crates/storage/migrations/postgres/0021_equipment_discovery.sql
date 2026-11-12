-- A device a source reports that matches no inverter of the system becomes a
-- proposal instead of equipment. The owner adopts it, which creates the
-- inverter and its strings, or dismisses it, after which the same device is
-- not proposed again until the dismissal is reset.
CREATE TABLE account_data.equipment_discovery_proposals (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    device_serial TEXT NOT NULL CHECK (char_length(device_serial) > 0),
    reported_name TEXT,
    model TEXT,
    firmware TEXT,
    state TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'dismissed', 'adopted')),
    -- The inverter an adopted proposal created.
    inverter_id UUID,
    first_seen_at BIGINT NOT NULL,
    last_seen_at BIGINT NOT NULL,
    decided_at BIGINT,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, system_id, device_serial),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);

-- A DC input is proposed as a string only once its power channel has reported
-- something other than zero, so an unused input never becomes a string.
ALTER TABLE account_data.channel_definitions ADD COLUMN first_nonzero_at BIGINT;

UPDATE account_data.channel_definitions c SET first_nonzero_at = COALESCE(c.last_value_at, c.updated_at)
WHERE EXISTS (
    SELECT 1 FROM telemetry.hot_extended_values v
    WHERE v.account_id = c.account_id AND v.channel_id = c.id AND v.integer_value <> 0
);
