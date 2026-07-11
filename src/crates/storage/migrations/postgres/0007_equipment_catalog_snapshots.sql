ALTER TABLE account_data.inverters
    ADD COLUMN catalog_entry_id TEXT,
    ADD COLUMN catalog_revision TEXT,
    ADD COLUMN value_provenance TEXT NOT NULL DEFAULT 'manual'
        CHECK (value_provenance IN ('manual', 'catalog_copied', 'catalog_customized')),
    ADD COLUMN specification_snapshot JSONB;

ALTER TABLE account_data.pv_strings
    ADD COLUMN module_catalog_entry_id TEXT,
    ADD COLUMN module_catalog_revision TEXT,
    ADD COLUMN value_provenance TEXT NOT NULL DEFAULT 'manual'
        CHECK (value_provenance IN ('manual', 'catalog_copied', 'catalog_customized')),
    ADD COLUMN module_specification_snapshot JSONB,
    ADD COLUMN module_peak_power_watts BIGINT
        CHECK (module_peak_power_watts IS NULL OR module_peak_power_watts > 0),
    ADD COLUMN total_peak_power_watts BIGINT
        CHECK (total_peak_power_watts IS NULL OR total_peak_power_watts > 0),
    ADD CONSTRAINT pv_strings_module_composition_check CHECK (
        module_peak_power_watts IS NULL
        OR (
            total_peak_power_watts = panel_count::BIGINT * module_peak_power_watts
            AND rated_power_watts = total_peak_power_watts
        )
    );
