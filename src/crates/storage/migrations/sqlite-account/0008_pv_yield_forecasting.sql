CREATE TABLE pv_string_forecast_settings (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    string_id BLOB NOT NULL REFERENCES pv_strings(id) ON DELETE CASCADE CHECK (length(string_id) = 16),
    effective_from INTEGER NOT NULL,
    effective_to INTEGER,
    model_identifier TEXT NOT NULL CHECK (length(trim(model_identifier)) > 0),
    model_revision INTEGER NOT NULL CHECK (model_revision > 0),
    soiling_loss_basis_points INTEGER NOT NULL CHECK (soiling_loss_basis_points BETWEEN 0 AND 10000),
    shading_loss_basis_points INTEGER NOT NULL CHECK (shading_loss_basis_points BETWEEN 0 AND 10000),
    mismatch_loss_basis_points INTEGER NOT NULL CHECK (mismatch_loss_basis_points BETWEEN 0 AND 10000),
    wiring_loss_basis_points INTEGER NOT NULL CHECK (wiring_loss_basis_points BETWEEN 0 AND 10000),
    unavailability_loss_basis_points INTEGER NOT NULL CHECK (unavailability_loss_basis_points BETWEEN 0 AND 10000),
    calibration_basis_points INTEGER NOT NULL CHECK (calibration_basis_points BETWEEN -10000 AND 10000),
    configuration_digest BLOB NOT NULL CHECK (length(configuration_digest) = 32),
    created_at INTEGER NOT NULL,
    created_by BLOB CHECK (created_by IS NULL OR length(created_by) = 16),
    CHECK (effective_to IS NULL OR effective_to > effective_from),
    UNIQUE (string_id, effective_from),
    UNIQUE (string_id, configuration_digest)
) STRICT;

CREATE INDEX pv_string_forecast_settings_effective_idx
    ON pv_string_forecast_settings(system_id, string_id, effective_from, effective_to);

CREATE TABLE weather_data_runs (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    provider_configuration_id BLOB NOT NULL REFERENCES provider_configurations(id) ON DELETE RESTRICT CHECK (length(provider_configuration_id) = 16),
    source_run_key TEXT NOT NULL CHECK (length(trim(source_run_key)) > 0),
    data_kind TEXT NOT NULL CHECK (data_kind IN ('forecast', 'observed', 'reanalysis')),
    issued_at INTEGER,
    fetched_at INTEGER NOT NULL,
    valid_from INTEGER NOT NULL,
    valid_to INTEGER NOT NULL,
    resolution_seconds INTEGER NOT NULL CHECK (resolution_seconds > 0),
    spatial_kind TEXT NOT NULL CHECK (spatial_kind IN ('point', 'provider_region')),
    latitude_e6 INTEGER CHECK (latitude_e6 IS NULL OR latitude_e6 BETWEEN -90000000 AND 90000000),
    longitude_e6 INTEGER CHECK (longitude_e6 IS NULL OR longitude_e6 BETWEEN -180000000 AND 180000000),
    provider_region TEXT,
    adapter TEXT NOT NULL CHECK (length(trim(adapter)) > 0),
    source_url TEXT NOT NULL,
    license_identifier TEXT NOT NULL CHECK (length(trim(license_identifier)) > 0),
    attribution TEXT NOT NULL CHECK (length(trim(attribution)) > 0),
    provenance_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(provenance_json)),
    retention_class TEXT NOT NULL DEFAULT 'working' CHECK (retention_class IN ('working', 'issued', 'referenced')),
    retain_until INTEGER,
    referenced_at INTEGER,
    created_at INTEGER NOT NULL,
    CHECK (valid_to > valid_from),
    CHECK (data_kind <> 'forecast' OR issued_at IS NOT NULL),
    CHECK (
        (spatial_kind = 'point' AND latitude_e6 IS NOT NULL AND longitude_e6 IS NOT NULL AND provider_region IS NULL)
        OR
        (spatial_kind = 'provider_region' AND latitude_e6 IS NULL AND longitude_e6 IS NULL AND provider_region IS NOT NULL)
    ),
    UNIQUE (provider_configuration_id, source_run_key)
) STRICT;

CREATE INDEX weather_data_runs_selection_idx
    ON weather_data_runs(system_id, data_kind, issued_at DESC, valid_from, valid_to);

CREATE INDEX weather_data_runs_retention_idx
    ON weather_data_runs(retention_class, retain_until, referenced_at);

CREATE TABLE weather_data_points (
    run_id BLOB NOT NULL REFERENCES weather_data_runs(id) ON DELETE CASCADE CHECK (length(run_id) = 16),
    interval_start INTEGER NOT NULL,
    interval_end INTEGER NOT NULL,
    global_horizontal_wm2 INTEGER CHECK (global_horizontal_wm2 IS NULL OR global_horizontal_wm2 >= 0),
    global_horizontal_lower_wm2 INTEGER CHECK (global_horizontal_lower_wm2 IS NULL OR global_horizontal_lower_wm2 >= 0),
    global_horizontal_upper_wm2 INTEGER CHECK (global_horizontal_upper_wm2 IS NULL OR global_horizontal_upper_wm2 >= 0),
    direct_normal_wm2 INTEGER CHECK (direct_normal_wm2 IS NULL OR direct_normal_wm2 >= 0),
    direct_normal_lower_wm2 INTEGER CHECK (direct_normal_lower_wm2 IS NULL OR direct_normal_lower_wm2 >= 0),
    direct_normal_upper_wm2 INTEGER CHECK (direct_normal_upper_wm2 IS NULL OR direct_normal_upper_wm2 >= 0),
    diffuse_horizontal_wm2 INTEGER CHECK (diffuse_horizontal_wm2 IS NULL OR diffuse_horizontal_wm2 >= 0),
    diffuse_horizontal_lower_wm2 INTEGER CHECK (diffuse_horizontal_lower_wm2 IS NULL OR diffuse_horizontal_lower_wm2 >= 0),
    diffuse_horizontal_upper_wm2 INTEGER CHECK (diffuse_horizontal_upper_wm2 IS NULL OR diffuse_horizontal_upper_wm2 >= 0),
    plane_of_array_wm2 INTEGER CHECK (plane_of_array_wm2 IS NULL OR plane_of_array_wm2 >= 0),
    plane_of_array_lower_wm2 INTEGER CHECK (plane_of_array_lower_wm2 IS NULL OR plane_of_array_lower_wm2 >= 0),
    plane_of_array_upper_wm2 INTEGER CHECK (plane_of_array_upper_wm2 IS NULL OR plane_of_array_upper_wm2 >= 0),
    ambient_temperature_millicelsius INTEGER,
    wind_speed_millimetres_per_second INTEGER CHECK (wind_speed_millimetres_per_second IS NULL OR wind_speed_millimetres_per_second >= 0),
    cloud_cover_basis_points INTEGER CHECK (cloud_cover_basis_points IS NULL OR cloud_cover_basis_points BETWEEN 0 AND 10000),
    PRIMARY KEY (run_id, interval_start),
    CHECK (interval_end > interval_start),
    CHECK (
        plane_of_array_wm2 IS NOT NULL
        OR global_horizontal_wm2 IS NOT NULL
        OR (direct_normal_wm2 IS NOT NULL AND diffuse_horizontal_wm2 IS NOT NULL)
    ),
    CHECK (global_horizontal_lower_wm2 IS NULL OR global_horizontal_wm2 IS NOT NULL),
    CHECK (global_horizontal_upper_wm2 IS NULL OR global_horizontal_wm2 IS NOT NULL),
    CHECK (plane_of_array_lower_wm2 IS NULL OR plane_of_array_wm2 IS NOT NULL),
    CHECK (plane_of_array_upper_wm2 IS NULL OR plane_of_array_wm2 IS NOT NULL)
) STRICT, WITHOUT ROWID;

CREATE TABLE yield_calculation_runs (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    weather_run_id BLOB NOT NULL REFERENCES weather_data_runs(id) ON DELETE RESTRICT CHECK (length(weather_run_id) = 16),
    basis TEXT NOT NULL CHECK (basis IN ('forecast', 'expected')),
    model_identifier TEXT NOT NULL CHECK (length(trim(model_identifier)) > 0),
    model_revision INTEGER NOT NULL CHECK (model_revision > 0),
    configuration_digest BLOB NOT NULL CHECK (length(configuration_digest) = 32),
    state TEXT NOT NULL CHECK (state IN ('pending', 'running', 'completed', 'failed', 'superseded')),
    requested_at INTEGER NOT NULL,
    completed_at INTEGER,
    safe_error_code TEXT,
    retention_class TEXT NOT NULL DEFAULT 'working' CHECK (retention_class IN ('working', 'issued', 'referenced')),
    retain_until INTEGER,
    referenced_at INTEGER,
    idempotency_key TEXT NOT NULL,
    UNIQUE (system_id, idempotency_key)
) STRICT;

CREATE INDEX yield_calculation_runs_selection_idx
    ON yield_calculation_runs(system_id, basis, state, requested_at DESC);

CREATE TABLE yield_calculation_results (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    calculation_run_id BLOB NOT NULL REFERENCES yield_calculation_runs(id) ON DELETE CASCADE CHECK (length(calculation_run_id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('string', 'inverter', 'system')),
    scope_id BLOB NOT NULL CHECK (length(scope_id) = 16),
    interval_start INTEGER NOT NULL,
    interval_end INTEGER NOT NULL,
    configuration_digest BLOB NOT NULL CHECK (length(configuration_digest) = 32),
    power_central_watts INTEGER,
    power_lower_watts INTEGER,
    power_upper_watts INTEGER,
    energy_central_wh INTEGER,
    energy_lower_wh INTEGER,
    energy_upper_wh INTEGER,
    included_capacity_watts INTEGER NOT NULL CHECK (included_capacity_watts >= 0),
    total_effective_capacity_watts INTEGER NOT NULL CHECK (total_effective_capacity_watts >= included_capacity_watts),
    completeness TEXT NOT NULL CHECK (completeness IN ('complete', 'partial', 'unavailable')),
    incomplete_reasons_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(incomplete_reasons_json)),
    uncertainty_known INTEGER NOT NULL CHECK (uncertainty_known IN (0, 1)),
    created_at INTEGER NOT NULL,
    CHECK (interval_end > interval_start),
    CHECK (power_lower_watts IS NULL OR power_central_watts IS NOT NULL),
    CHECK (power_upper_watts IS NULL OR power_central_watts IS NOT NULL),
    CHECK (energy_lower_wh IS NULL OR energy_central_wh IS NOT NULL),
    CHECK (energy_upper_wh IS NULL OR energy_central_wh IS NOT NULL),
    UNIQUE (calculation_run_id, scope_kind, scope_id, interval_start)
) STRICT;

CREATE INDEX yield_calculation_results_range_idx
    ON yield_calculation_results(system_id, scope_kind, scope_id, interval_start, interval_end);

CREATE TABLE yield_result_projections (
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    basis TEXT NOT NULL CHECK (basis IN ('forecast', 'expected')),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('string', 'inverter', 'system')),
    scope_id BLOB NOT NULL CHECK (length(scope_id) = 16),
    interval_start INTEGER NOT NULL,
    result_id BLOB NOT NULL REFERENCES yield_calculation_results(id) ON DELETE CASCADE CHECK (length(result_id) = 16),
    projected_at INTEGER NOT NULL,
    PRIMARY KEY (system_id, basis, scope_kind, scope_id, interval_start)
) STRICT, WITHOUT ROWID;

CREATE TABLE yield_invalidations (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    range_start INTEGER NOT NULL,
    range_end INTEGER NOT NULL,
    reason TEXT NOT NULL CHECK (reason IN ('equipment', 'settings', 'provider_revision', 'late_telemetry', 'correction', 'model_version')),
    state TEXT NOT NULL CHECK (state IN ('pending', 'leased', 'completed', 'failed')),
    idempotency_key TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    completed_at INTEGER,
    CHECK (range_end > range_start),
    UNIQUE (system_id, idempotency_key)
) STRICT;

CREATE INDEX yield_invalidations_dispatch_idx
    ON yield_invalidations(state, system_id, range_start, range_end);

CREATE TRIGGER weather_data_runs_no_update
BEFORE UPDATE ON weather_data_runs
BEGIN
    SELECT RAISE(ABORT, 'weather data runs are immutable');
END;

CREATE TRIGGER weather_data_points_no_update
BEFORE UPDATE ON weather_data_points
BEGIN
    SELECT RAISE(ABORT, 'weather data points are immutable');
END;

CREATE TRIGGER yield_calculation_results_no_update
BEFORE UPDATE ON yield_calculation_results
BEGIN
    SELECT RAISE(ABORT, 'yield calculation results are immutable');
END;
