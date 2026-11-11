-- A system can be created from a name alone: capacity may be declared before any
-- string exists, and the location keeps the label the owner picked next to its
-- coordinates. Strings keep the loss inputs their editor asks for.

ALTER TABLE account_data.systems
    ADD COLUMN declared_capacity_watts BIGINT
        CHECK (declared_capacity_watts IS NULL OR declared_capacity_watts > 0),
    ADD COLUMN location_label TEXT
        CHECK (location_label IS NULL OR length(btrim(location_label)) > 0);

ALTER TABLE account_data.pv_strings
    ADD COLUMN temperature_coefficient_ppm_per_celsius INTEGER
        CHECK (temperature_coefficient_ppm_per_celsius IS NULL
            OR temperature_coefficient_ppm_per_celsius BETWEEN -20000 AND 0),
    ADD COLUMN shading JSONB NOT NULL DEFAULT '[]'::jsonb
        CHECK (jsonb_typeof(shading) = 'array');
