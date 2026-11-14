-- Tables keyed by their primary key alone are stored as that key, instead of
-- a row id table plus a second b-tree holding the key again.

-- A value no longer references its channel: checking that reference when a
-- channel definition goes needs an index over every value by channel. Channel
-- definitions are retired, never deleted, except together with their system,
-- whose readings take their values with them.

CREATE TABLE telemetry_hot_extended_values_compact (
    observation_id BLOB NOT NULL REFERENCES telemetry_hot(observation_id) ON DELETE CASCADE CHECK (length(observation_id) = 16),
    channel_id BLOB NOT NULL CHECK (length(channel_id) = 16),
    integer_value INTEGER NOT NULL,
    PRIMARY KEY (observation_id, channel_id)
) STRICT, WITHOUT ROWID;
INSERT INTO telemetry_hot_extended_values_compact (observation_id, channel_id, integer_value)
    SELECT observation_id, channel_id, integer_value FROM telemetry_hot_extended_values
    ORDER BY observation_id, channel_id;
DROP TABLE telemetry_hot_extended_values;
ALTER TABLE telemetry_hot_extended_values_compact RENAME TO telemetry_hot_extended_values;

-- A target's channel rollups are found through the channels bound to it, so
-- they need no index of their own.
CREATE TABLE channel_rollups_compact (
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    channel_id BLOB NOT NULL REFERENCES channel_definitions(id) ON DELETE CASCADE CHECK (length(channel_id) = 16),
    target_kind TEXT NOT NULL CHECK (target_kind IN ('plant', 'inverter', 'string')),
    target_id BLOB CHECK (target_id IS NULL OR length(target_id) = 16),
    resolution TEXT NOT NULL CHECK (resolution IN ('fifteen_minute', 'hour', 'day', 'month', 'year')),
    bucket_start INTEGER NOT NULL,
    bucket_end INTEGER NOT NULL,
    point_count INTEGER NOT NULL CHECK (point_count > 0),
    value_sum INTEGER NOT NULL,
    value_min INTEGER NOT NULL,
    value_max INTEGER NOT NULL,
    value_first INTEGER NOT NULL,
    value_last INTEGER NOT NULL,
    -- Growth of a counter within the bucket. Absent for other channels and
    -- for a bucket whose first value has no predecessor.
    counter_delta INTEGER,
    calculated_at INTEGER NOT NULL,
    PRIMARY KEY (channel_id, resolution, bucket_start),
    CHECK (bucket_end > bucket_start),
    CHECK ((target_kind = 'plant') = (target_id IS NULL))
) STRICT, WITHOUT ROWID;
INSERT INTO channel_rollups_compact
    SELECT system_id, channel_id, target_kind, target_id, resolution, bucket_start, bucket_end,
        point_count, value_sum, value_min, value_max, value_first, value_last, counter_delta,
        calculated_at
    FROM channel_rollups ORDER BY channel_id, resolution, bucket_start;
DROP TABLE channel_rollups;
ALTER TABLE channel_rollups_compact RENAME TO channel_rollups;

CREATE TABLE telemetry_rollups_compact (
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    scope_kind TEXT NOT NULL DEFAULT 'system' CHECK (scope_kind IN ('system', 'inverter', 'string')),
    scope_id BLOB NOT NULL CHECK (length(scope_id) = 16),
    resolution TEXT NOT NULL CHECK (resolution IN ('fifteen_minute', 'hour', 'day', 'month', 'year')),
    bucket_start INTEGER NOT NULL,
    bucket_end INTEGER NOT NULL,
    timezone TEXT NOT NULL,
    generation INTEGER NOT NULL DEFAULT 1 CHECK (generation > 0),
    source_generation INTEGER NOT NULL DEFAULT 1 CHECK (source_generation > 0),
    point_count INTEGER NOT NULL CHECK (point_count >= 0),
    expected_count INTEGER NOT NULL CHECK (expected_count >= 0),
    generation_energy_sum_wh INTEGER,
    generation_power_min_watts INTEGER,
    generation_power_max_watts INTEGER,
    generation_power_first_watts INTEGER,
    generation_power_last_watts INTEGER,
    consumption_energy_sum_wh INTEGER,
    grid_import_energy_sum_wh INTEGER,
    grid_export_energy_sum_wh INTEGER,
    battery_energy_delta_wh INTEGER,
    temperature_min_millidegrees_c INTEGER,
    temperature_max_millidegrees_c INTEGER,
    quality_flags INTEGER NOT NULL DEFAULT 0 CHECK (quality_flags BETWEEN 0 AND 65535),
    coverage_basis_points INTEGER NOT NULL CHECK (coverage_basis_points BETWEEN 0 AND 10000),
    calculated_at INTEGER NOT NULL,
    voltage_mean_millivolts INTEGER,
    PRIMARY KEY (system_id, scope_kind, scope_id, resolution, bucket_start, generation),
    CHECK (bucket_end > bucket_start),
    CHECK (point_count <= expected_count OR expected_count = 0)
) STRICT, WITHOUT ROWID;
INSERT INTO telemetry_rollups_compact
    SELECT system_id, scope_kind, scope_id, resolution, bucket_start, bucket_end, timezone,
        generation, source_generation, point_count, expected_count, generation_energy_sum_wh,
        generation_power_min_watts, generation_power_max_watts, generation_power_first_watts,
        generation_power_last_watts, consumption_energy_sum_wh, grid_import_energy_sum_wh,
        grid_export_energy_sum_wh, battery_energy_delta_wh, temperature_min_millidegrees_c,
        temperature_max_millidegrees_c, quality_flags, coverage_basis_points, calculated_at,
        voltage_mean_millivolts
    FROM telemetry_rollups ORDER BY system_id, scope_kind, scope_id, resolution, bucket_start, generation;
DROP TABLE telemetry_rollups;
ALTER TABLE telemetry_rollups_compact RENAME TO telemetry_rollups;
-- The key already orders a scope's buckets, so only these two remain.
CREATE INDEX telemetry_rollups_query_idx
    ON telemetry_rollups(system_id, resolution, bucket_start, bucket_end);
CREATE INDEX telemetry_rollups_system_end_idx ON telemetry_rollups(system_id, bucket_end);

-- Every query by receive time names the system, which the system's receive
-- time index answers.
DROP INDEX telemetry_hot_received_idx;
