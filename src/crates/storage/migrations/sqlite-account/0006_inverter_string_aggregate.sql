-- Generation equipment is owned through the system -> inverter -> PV string aggregate.
CREATE TABLE inverters (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    manufacturer TEXT,
    model TEXT,
    serial_reference TEXT,
    rated_power_watts INTEGER CHECK (rated_power_watts IS NULL OR rated_power_watts > 0),
    effective_from INTEGER NOT NULL,
    effective_to INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    CHECK (effective_to IS NULL OR effective_to > effective_from)
) STRICT;

CREATE INDEX inverters_system_effective_idx
    ON inverters(system_id, effective_from, effective_to, id);

CREATE TABLE pv_strings (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    inverter_id BLOB NOT NULL REFERENCES inverters(id) ON DELETE CASCADE CHECK (length(inverter_id) = 16),
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    panel_count INTEGER NOT NULL CHECK (panel_count > 0),
    panel_manufacturer TEXT,
    panel_model TEXT,
    rated_power_watts INTEGER NOT NULL CHECK (rated_power_watts > 0),
    orientation_degrees INTEGER CHECK (orientation_degrees IS NULL OR orientation_degrees BETWEEN 0 AND 359),
    tilt_degrees INTEGER CHECK (tilt_degrees IS NULL OR tilt_degrees BETWEEN 0 AND 90),
    effective_from INTEGER NOT NULL,
    effective_to INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    CHECK (effective_to IS NULL OR effective_to > effective_from)
) STRICT;

CREATE INDEX pv_strings_inverter_effective_idx
    ON pv_strings(inverter_id, effective_from, effective_to, id);

CREATE TRIGGER equipment_generation_kind_insert_guard
BEFORE INSERT ON equipment
WHEN NEW.equipment_kind IN ('array', 'inverter')
BEGIN
    SELECT RAISE(ABORT, 'generation equipment must use the inverter/string aggregate');
END;

CREATE TRIGGER equipment_generation_kind_update_guard
BEFORE UPDATE OF equipment_kind ON equipment
WHEN NEW.equipment_kind IN ('array', 'inverter')
BEGIN
    SELECT RAISE(ABORT, 'generation equipment must use the inverter/string aggregate');
END;
