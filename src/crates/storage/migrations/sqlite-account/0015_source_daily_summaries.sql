-- Daily summaries as a source reports them, kept next to the summaries PVLog
-- computes from interval readings. A source's own daily counter is often more
-- accurate than integrated samples, and for older history it may be all there is.
--
-- One row per source, device and local date; resubmitting replaces the row.
-- device_key is empty for a summary of the whole system.
CREATE TABLE source_daily_summaries (
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    source_key TEXT NOT NULL,
    device_key TEXT NOT NULL DEFAULT '',
    local_date TEXT NOT NULL CHECK (local_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    data_source_id BLOB CHECK (data_source_id IS NULL OR length(data_source_id) = 16),
    generation_energy_wh INTEGER CHECK (generation_energy_wh IS NULL OR generation_energy_wh >= 0),
    export_energy_wh INTEGER CHECK (export_energy_wh IS NULL OR export_energy_wh >= 0),
    consumption_energy_wh INTEGER CHECK (consumption_energy_wh IS NULL OR consumption_energy_wh >= 0),
    generation_lifetime_wh INTEGER CHECK (generation_lifetime_wh IS NULL OR generation_lifetime_wh >= 0),
    complete INTEGER NOT NULL DEFAULT 0 CHECK (complete IN (0, 1)),
    peak_power_watts INTEGER CHECK (peak_power_watts IS NULL OR peak_power_watts >= 0),
    peak_time TEXT CHECK (peak_time IS NULL OR peak_time GLOB '[0-2][0-9]:[0-5][0-9]'),
    weather_condition TEXT CHECK (weather_condition IS NULL OR length(weather_condition) <= 40),
    temperature_min_millidegrees_c INTEGER,
    temperature_max_millidegrees_c INTEGER,
    import_energy_by_band_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(import_energy_by_band_json)),
    received_at INTEGER NOT NULL,
    PRIMARY KEY (system_id, source_key, device_key, local_date)
) STRICT;

CREATE INDEX source_daily_summaries_date_idx
    ON source_daily_summaries(system_id, local_date);
