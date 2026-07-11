-- Generation equipment is owned through the system -> inverter -> PV string aggregate.
CREATE TABLE account_data.inverters (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    name TEXT NOT NULL CHECK (length(btrim(name)) > 0),
    manufacturer TEXT,
    model TEXT,
    serial_reference TEXT,
    rated_power_watts BIGINT CHECK (rated_power_watts IS NULL OR rated_power_watts > 0),
    effective_from BIGINT NOT NULL,
    effective_to BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    PRIMARY KEY (account_id, id),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    CHECK (effective_to IS NULL OR effective_to > effective_from)
);

CREATE INDEX inverters_system_effective_idx
    ON account_data.inverters(account_id, system_id, effective_from, effective_to, id);

CREATE TABLE account_data.pv_strings (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    inverter_id UUID NOT NULL,
    name TEXT NOT NULL CHECK (length(btrim(name)) > 0),
    panel_count INTEGER NOT NULL CHECK (panel_count > 0),
    panel_manufacturer TEXT,
    panel_model TEXT,
    rated_power_watts BIGINT NOT NULL CHECK (rated_power_watts > 0),
    orientation_degrees INTEGER CHECK (orientation_degrees IS NULL OR orientation_degrees BETWEEN 0 AND 359),
    tilt_degrees INTEGER CHECK (tilt_degrees IS NULL OR tilt_degrees BETWEEN 0 AND 90),
    effective_from BIGINT NOT NULL,
    effective_to BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    PRIMARY KEY (account_id, id),
    FOREIGN KEY (account_id, inverter_id)
        REFERENCES account_data.inverters(account_id, id) ON DELETE CASCADE,
    CHECK (effective_to IS NULL OR effective_to > effective_from)
);

CREATE INDEX pv_strings_inverter_effective_idx
    ON account_data.pv_strings(account_id, inverter_id, effective_from, effective_to, id);

ALTER TABLE account_data.equipment
    DROP CONSTRAINT equipment_equipment_kind_check;
ALTER TABLE account_data.equipment
    ADD CONSTRAINT equipment_equipment_kind_check
    CHECK (equipment_kind IN ('meter', 'battery', 'sensor', 'other'));
