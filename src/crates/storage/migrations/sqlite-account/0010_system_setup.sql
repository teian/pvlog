-- A system can be created from a name alone: capacity may be declared before any
-- string exists, and the location keeps the label the owner picked next to its
-- coordinates. Strings keep the loss inputs their editor asks for.

ALTER TABLE systems ADD COLUMN declared_capacity_watts INTEGER
    CHECK (declared_capacity_watts IS NULL OR declared_capacity_watts > 0);
ALTER TABLE systems ADD COLUMN location_label TEXT
    CHECK (location_label IS NULL OR length(trim(location_label)) > 0);

ALTER TABLE pv_strings ADD COLUMN temperature_coefficient_ppm_per_celsius INTEGER
    CHECK (temperature_coefficient_ppm_per_celsius IS NULL
        OR temperature_coefficient_ppm_per_celsius BETWEEN -20000 AND 0);
ALTER TABLE pv_strings ADD COLUMN shading_json TEXT NOT NULL DEFAULT '[]'
    CHECK (json_valid(shading_json) AND json_type(shading_json) = 'array');
