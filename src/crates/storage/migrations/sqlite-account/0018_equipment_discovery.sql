-- A device a source reports that matches no inverter of the system becomes a
-- proposal instead of equipment. The owner adopts it, which creates the
-- inverter and its strings, or dismisses it, after which the same device is
-- not proposed again until the dismissal is reset.
CREATE TABLE equipment_discovery_proposals (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    device_serial TEXT NOT NULL CHECK (length(device_serial) > 0),
    reported_name TEXT,
    model TEXT,
    firmware TEXT,
    state TEXT NOT NULL DEFAULT 'open' CHECK (state IN ('open', 'dismissed', 'adopted')),
    -- The inverter an adopted proposal created.
    inverter_id BLOB CHECK (inverter_id IS NULL OR length(inverter_id) = 16),
    first_seen_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    decided_at INTEGER,
    UNIQUE (system_id, device_serial)
) STRICT;

-- A DC input is proposed as a string only once its power channel has reported
-- something other than zero, so an unused input never becomes a string.
ALTER TABLE channel_definitions ADD COLUMN first_nonzero_at INTEGER;

UPDATE channel_definitions SET first_nonzero_at = COALESCE(last_value_at, updated_at)
WHERE EXISTS (
    SELECT 1 FROM telemetry_hot_extended_values v
    WHERE v.channel_id = channel_definitions.id AND v.integer_value <> 0
);
