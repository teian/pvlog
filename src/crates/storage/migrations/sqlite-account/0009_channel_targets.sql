-- Channel targets bind an ingested measurement key to the equipment it meters,
-- and rollups gain the scope hierarchy the yield projections already carry.
--
-- Readings arrive under source-defined keys ("mppt-1", "wr1.mppt2"). Until an
-- operator binds a key to a string, inverter, or the plant, the key is retained
-- as 'unassigned' so its values are never silently discarded.

ALTER TABLE channel_definitions ADD COLUMN target_kind TEXT NOT NULL DEFAULT 'unassigned'
    CHECK (target_kind IN ('unassigned', 'plant', 'inverter', 'string'));
ALTER TABLE channel_definitions ADD COLUMN target_id BLOB
    CHECK (target_id IS NULL OR length(target_id) = 16);
ALTER TABLE channel_definitions ADD COLUMN tracks_power INTEGER NOT NULL DEFAULT 1
    CHECK (tracks_power IN (0, 1));
ALTER TABLE channel_definitions ADD COLUMN tracks_energy INTEGER NOT NULL DEFAULT 0
    CHECK (tracks_energy IN (0, 1));
ALTER TABLE channel_definitions ADD COLUMN discovered_at INTEGER;
ALTER TABLE channel_definitions ADD COLUMN last_value_at INTEGER;

CREATE INDEX channel_definitions_target_idx
    ON channel_definitions(system_id, target_kind, target_id);

-- A bound channel must name its equipment; the plant target is the system itself.
CREATE TRIGGER channel_definitions_target_insert_guard
BEFORE INSERT ON channel_definitions
BEGIN
    SELECT CASE
        WHEN NEW.target_kind IN ('inverter', 'string') AND NEW.target_id IS NULL
        THEN RAISE(ABORT, 'channel target requires an equipment identifier')
        WHEN NEW.target_kind IN ('unassigned', 'plant') AND NEW.target_id IS NOT NULL
        THEN RAISE(ABORT, 'channel target does not accept an equipment identifier')
    END;
END;

CREATE TRIGGER channel_definitions_target_update_guard
BEFORE UPDATE OF target_kind, target_id ON channel_definitions
BEGIN
    SELECT CASE
        WHEN NEW.target_kind IN ('inverter', 'string') AND NEW.target_id IS NULL
        THEN RAISE(ABORT, 'channel target requires an equipment identifier')
        WHEN NEW.target_kind IN ('unassigned', 'plant') AND NEW.target_id IS NOT NULL
        THEN RAISE(ABORT, 'channel target does not accept an equipment identifier')
    END;
END;

-- telemetry_rollups is rebuilt rather than altered: the scope belongs in the
-- primary key, otherwise a string bucket and the system bucket covering the same
-- instant would collide. Following yield_result_projections, scope_id is never
-- null and carries the system identifier for the system scope.
CREATE TABLE telemetry_rollups_scoped (
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
    PRIMARY KEY (system_id, scope_kind, scope_id, resolution, bucket_start, generation),
    CHECK (bucket_end > bucket_start),
    CHECK (point_count <= expected_count OR expected_count = 0)
) STRICT;

INSERT INTO telemetry_rollups_scoped (
    system_id, scope_kind, scope_id, resolution, bucket_start, bucket_end, timezone,
    generation, source_generation, point_count, expected_count,
    generation_energy_sum_wh, generation_power_min_watts, generation_power_max_watts,
    generation_power_first_watts, generation_power_last_watts, consumption_energy_sum_wh,
    grid_import_energy_sum_wh, grid_export_energy_sum_wh, battery_energy_delta_wh,
    temperature_min_millidegrees_c, temperature_max_millidegrees_c,
    quality_flags, coverage_basis_points, calculated_at
)
SELECT
    system_id, 'system', system_id, resolution, bucket_start, bucket_end, timezone,
    generation, source_generation, point_count, expected_count,
    generation_energy_sum_wh, generation_power_min_watts, generation_power_max_watts,
    generation_power_first_watts, generation_power_last_watts, consumption_energy_sum_wh,
    grid_import_energy_sum_wh, grid_export_energy_sum_wh, battery_energy_delta_wh,
    temperature_min_millidegrees_c, temperature_max_millidegrees_c,
    quality_flags, coverage_basis_points, calculated_at
FROM telemetry_rollups;

DROP TABLE telemetry_rollups;
ALTER TABLE telemetry_rollups_scoped RENAME TO telemetry_rollups;

CREATE INDEX telemetry_rollups_query_idx
    ON telemetry_rollups(system_id, resolution, bucket_start, bucket_end);
CREATE INDEX telemetry_rollups_scope_idx
    ON telemetry_rollups(system_id, scope_kind, scope_id, resolution, bucket_start);
