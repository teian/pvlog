-- Daily summaries as a source reports them, kept next to the summaries PVLog
-- computes from interval readings. A source's own daily counter is often more
-- accurate than integrated samples, and for older history it may be all there is.
--
-- One row per source, device and local date; resubmitting replaces the row.
-- device_key is empty for a summary of the whole system.
CREATE TABLE telemetry.source_daily_summaries (
    account_id UUID NOT NULL,
    system_id UUID NOT NULL,
    source_key TEXT NOT NULL,
    device_key TEXT NOT NULL DEFAULT '',
    local_date DATE NOT NULL,
    data_source_id UUID,
    generation_energy_wh BIGINT CHECK (generation_energy_wh IS NULL OR generation_energy_wh >= 0),
    export_energy_wh BIGINT CHECK (export_energy_wh IS NULL OR export_energy_wh >= 0),
    consumption_energy_wh BIGINT CHECK (consumption_energy_wh IS NULL OR consumption_energy_wh >= 0),
    generation_lifetime_wh BIGINT CHECK (generation_lifetime_wh IS NULL OR generation_lifetime_wh >= 0),
    complete BOOLEAN NOT NULL DEFAULT FALSE,
    peak_power_watts BIGINT CHECK (peak_power_watts IS NULL OR peak_power_watts >= 0),
    peak_time TEXT CHECK (peak_time IS NULL OR peak_time ~ '^[0-2][0-9]:[0-5][0-9]$'),
    weather_condition TEXT CHECK (weather_condition IS NULL OR char_length(weather_condition) <= 40),
    temperature_min_millidegrees_c BIGINT,
    temperature_max_millidegrees_c BIGINT,
    import_energy_by_band JSONB NOT NULL DEFAULT '{}'::jsonb,
    received_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, system_id, source_key, device_key, local_date),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);

CREATE INDEX source_daily_summaries_date_idx
    ON telemetry.source_daily_summaries(account_id, system_id, local_date);
