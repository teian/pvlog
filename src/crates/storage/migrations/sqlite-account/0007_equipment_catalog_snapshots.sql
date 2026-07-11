ALTER TABLE inverters ADD COLUMN catalog_entry_id TEXT;
ALTER TABLE inverters ADD COLUMN catalog_revision TEXT;
ALTER TABLE inverters ADD COLUMN value_provenance TEXT NOT NULL DEFAULT 'manual'
    CHECK (value_provenance IN ('manual', 'catalog_copied', 'catalog_customized'));
ALTER TABLE inverters ADD COLUMN specification_snapshot_json TEXT
    CHECK (specification_snapshot_json IS NULL OR json_valid(specification_snapshot_json));

ALTER TABLE pv_strings ADD COLUMN module_catalog_entry_id TEXT;
ALTER TABLE pv_strings ADD COLUMN module_catalog_revision TEXT;
ALTER TABLE pv_strings ADD COLUMN value_provenance TEXT NOT NULL DEFAULT 'manual'
    CHECK (value_provenance IN ('manual', 'catalog_copied', 'catalog_customized'));
ALTER TABLE pv_strings ADD COLUMN module_specification_snapshot_json TEXT
    CHECK (module_specification_snapshot_json IS NULL OR json_valid(module_specification_snapshot_json));
ALTER TABLE pv_strings ADD COLUMN module_peak_power_watts INTEGER
    CHECK (module_peak_power_watts IS NULL OR module_peak_power_watts > 0);
ALTER TABLE pv_strings ADD COLUMN total_peak_power_watts INTEGER
    CHECK (total_peak_power_watts IS NULL OR total_peak_power_watts > 0);

CREATE TRIGGER pv_strings_catalog_snapshot_insert_guard
BEFORE INSERT ON pv_strings
WHEN NEW.module_peak_power_watts IS NOT NULL
BEGIN
    SELECT CASE
        WHEN NEW.total_peak_power_watts IS NULL
          OR NEW.total_peak_power_watts <> NEW.panel_count * NEW.module_peak_power_watts
          OR NEW.rated_power_watts <> NEW.total_peak_power_watts
        THEN RAISE(ABORT, 'PV string total peak power does not match module composition')
    END;
END;

CREATE TRIGGER pv_strings_catalog_snapshot_update_guard
BEFORE UPDATE OF panel_count, module_peak_power_watts, total_peak_power_watts, rated_power_watts
ON pv_strings
WHEN NEW.module_peak_power_watts IS NOT NULL
BEGIN
    SELECT CASE
        WHEN NEW.total_peak_power_watts IS NULL
          OR NEW.total_peak_power_watts <> NEW.panel_count * NEW.module_peak_power_watts
          OR NEW.rated_power_watts <> NEW.total_peak_power_watts
        THEN RAISE(ABORT, 'PV string total peak power does not match module composition')
    END;
END;
