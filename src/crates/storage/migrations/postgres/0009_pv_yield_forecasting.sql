CREATE TABLE account_data.pv_string_forecast_settings (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    string_id UUID NOT NULL,
    effective_from BIGINT NOT NULL,
    effective_to BIGINT,
    model_identifier TEXT NOT NULL CHECK (length(btrim(model_identifier)) > 0),
    model_revision INTEGER NOT NULL CHECK (model_revision > 0),
    soiling_loss_basis_points INTEGER NOT NULL CHECK (soiling_loss_basis_points BETWEEN 0 AND 10000),
    shading_loss_basis_points INTEGER NOT NULL CHECK (shading_loss_basis_points BETWEEN 0 AND 10000),
    mismatch_loss_basis_points INTEGER NOT NULL CHECK (mismatch_loss_basis_points BETWEEN 0 AND 10000),
    wiring_loss_basis_points INTEGER NOT NULL CHECK (wiring_loss_basis_points BETWEEN 0 AND 10000),
    unavailability_loss_basis_points INTEGER NOT NULL CHECK (unavailability_loss_basis_points BETWEEN 0 AND 10000),
    calibration_basis_points INTEGER NOT NULL CHECK (calibration_basis_points BETWEEN -10000 AND 10000),
    configuration_digest BYTEA NOT NULL CHECK (octet_length(configuration_digest) = 32),
    created_at BIGINT NOT NULL,
    created_by UUID,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, string_id, effective_from),
    UNIQUE (account_id, string_id, configuration_digest),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, string_id)
        REFERENCES account_data.pv_strings(account_id, id) ON DELETE CASCADE,
    CHECK (effective_to IS NULL OR effective_to > effective_from)
);

CREATE INDEX pv_string_forecast_settings_effective_idx
    ON account_data.pv_string_forecast_settings(account_id, system_id, string_id, effective_from, effective_to);

CREATE TABLE account_data.weather_data_runs (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    provider_configuration_id UUID NOT NULL,
    source_run_key TEXT NOT NULL CHECK (length(btrim(source_run_key)) > 0),
    data_kind TEXT NOT NULL CHECK (data_kind IN ('forecast', 'observed', 'reanalysis')),
    issued_at BIGINT,
    fetched_at BIGINT NOT NULL,
    valid_from BIGINT NOT NULL,
    valid_to BIGINT NOT NULL,
    resolution_seconds INTEGER NOT NULL CHECK (resolution_seconds > 0),
    spatial_kind TEXT NOT NULL CHECK (spatial_kind IN ('point', 'provider_region')),
    latitude_e6 INTEGER CHECK (latitude_e6 IS NULL OR latitude_e6 BETWEEN -90000000 AND 90000000),
    longitude_e6 INTEGER CHECK (longitude_e6 IS NULL OR longitude_e6 BETWEEN -180000000 AND 180000000),
    provider_region TEXT,
    adapter TEXT NOT NULL CHECK (length(btrim(adapter)) > 0),
    source_url TEXT NOT NULL,
    license_identifier TEXT NOT NULL CHECK (length(btrim(license_identifier)) > 0),
    attribution TEXT NOT NULL CHECK (length(btrim(attribution)) > 0),
    provenance JSONB NOT NULL DEFAULT '{}'::jsonb,
    retention_class TEXT NOT NULL DEFAULT 'working' CHECK (retention_class IN ('working', 'issued', 'referenced')),
    retain_until BIGINT,
    referenced_at BIGINT,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, provider_configuration_id, source_run_key),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, provider_configuration_id)
        REFERENCES integrations.providers(account_id, id) ON DELETE RESTRICT,
    CHECK (valid_to > valid_from),
    CHECK (data_kind <> 'forecast' OR issued_at IS NOT NULL),
    CHECK (
        (spatial_kind = 'point' AND latitude_e6 IS NOT NULL AND longitude_e6 IS NOT NULL AND provider_region IS NULL)
        OR
        (spatial_kind = 'provider_region' AND latitude_e6 IS NULL AND longitude_e6 IS NULL AND provider_region IS NOT NULL)
    )
);

CREATE INDEX weather_data_runs_selection_idx
    ON account_data.weather_data_runs(account_id, system_id, data_kind, issued_at DESC, valid_from, valid_to);

CREATE INDEX weather_data_runs_retention_idx
    ON account_data.weather_data_runs(retention_class, retain_until, referenced_at);

CREATE TABLE account_data.weather_data_points (
    account_id UUID NOT NULL,
    run_id UUID NOT NULL,
    interval_start BIGINT NOT NULL,
    interval_end BIGINT NOT NULL,
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
    PRIMARY KEY (account_id, run_id, interval_start),
    FOREIGN KEY (account_id, run_id)
        REFERENCES account_data.weather_data_runs(account_id, id) ON DELETE CASCADE,
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
);

CREATE TABLE account_data.yield_calculation_runs (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    weather_run_id UUID NOT NULL,
    basis TEXT NOT NULL CHECK (basis IN ('forecast', 'expected')),
    model_identifier TEXT NOT NULL CHECK (length(btrim(model_identifier)) > 0),
    model_revision INTEGER NOT NULL CHECK (model_revision > 0),
    configuration_digest BYTEA NOT NULL CHECK (octet_length(configuration_digest) = 32),
    state TEXT NOT NULL CHECK (state IN ('pending', 'running', 'completed', 'failed', 'superseded')),
    requested_at BIGINT NOT NULL,
    completed_at BIGINT,
    safe_error_code TEXT,
    retention_class TEXT NOT NULL DEFAULT 'working' CHECK (retention_class IN ('working', 'issued', 'referenced')),
    retain_until BIGINT,
    referenced_at BIGINT,
    idempotency_key TEXT NOT NULL,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, system_id, idempotency_key),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, weather_run_id)
        REFERENCES account_data.weather_data_runs(account_id, id) ON DELETE RESTRICT
);

CREATE INDEX yield_calculation_runs_selection_idx
    ON account_data.yield_calculation_runs(account_id, system_id, basis, state, requested_at DESC);

CREATE TABLE account_data.yield_calculation_results (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    calculation_run_id UUID NOT NULL,
    system_id UUID NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('string', 'inverter', 'system')),
    scope_id UUID NOT NULL,
    interval_start BIGINT NOT NULL,
    interval_end BIGINT NOT NULL,
    configuration_digest BYTEA NOT NULL CHECK (octet_length(configuration_digest) = 32),
    power_central_watts BIGINT,
    power_lower_watts BIGINT,
    power_upper_watts BIGINT,
    energy_central_wh BIGINT,
    energy_lower_wh BIGINT,
    energy_upper_wh BIGINT,
    included_capacity_watts BIGINT NOT NULL CHECK (included_capacity_watts >= 0),
    total_effective_capacity_watts BIGINT NOT NULL CHECK (total_effective_capacity_watts >= included_capacity_watts),
    completeness TEXT NOT NULL CHECK (completeness IN ('complete', 'partial', 'unavailable')),
    incomplete_reasons JSONB NOT NULL DEFAULT '[]'::jsonb,
    uncertainty_known BOOLEAN NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, calculation_run_id, scope_kind, scope_id, interval_start),
    FOREIGN KEY (account_id, calculation_run_id)
        REFERENCES account_data.yield_calculation_runs(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    CHECK (interval_end > interval_start),
    CHECK (power_lower_watts IS NULL OR power_central_watts IS NOT NULL),
    CHECK (power_upper_watts IS NULL OR power_central_watts IS NOT NULL),
    CHECK (energy_lower_wh IS NULL OR energy_central_wh IS NOT NULL),
    CHECK (energy_upper_wh IS NULL OR energy_central_wh IS NOT NULL)
);

CREATE INDEX yield_calculation_results_range_idx
    ON account_data.yield_calculation_results(account_id, system_id, scope_kind, scope_id, interval_start, interval_end);

CREATE TABLE account_data.yield_result_projections (
    account_id UUID NOT NULL,
    system_id UUID NOT NULL,
    basis TEXT NOT NULL CHECK (basis IN ('forecast', 'expected')),
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('string', 'inverter', 'system')),
    scope_id UUID NOT NULL,
    interval_start BIGINT NOT NULL,
    result_id UUID NOT NULL,
    projected_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, system_id, basis, scope_kind, scope_id, interval_start),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, result_id)
        REFERENCES account_data.yield_calculation_results(account_id, id) ON DELETE CASCADE
);

CREATE TABLE account_data.yield_invalidations (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    range_start BIGINT NOT NULL,
    range_end BIGINT NOT NULL,
    reason TEXT NOT NULL CHECK (reason IN ('equipment', 'settings', 'provider_revision', 'late_telemetry', 'correction', 'model_version')),
    state TEXT NOT NULL CHECK (state IN ('pending', 'leased', 'completed', 'failed')),
    idempotency_key TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    completed_at BIGINT,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, system_id, idempotency_key),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    CHECK (range_end > range_start)
);

CREATE INDEX yield_invalidations_dispatch_idx
    ON account_data.yield_invalidations(state, account_id, system_id, range_start, range_end);

CREATE FUNCTION account_data.reject_immutable_yield_update() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'yield forecast input and result rows are immutable';
END;
$$;

CREATE TRIGGER weather_data_runs_no_update
BEFORE UPDATE OF id, account_id, system_id, provider_configuration_id, source_run_key,
    data_kind, issued_at, fetched_at, valid_from, valid_to, resolution_seconds,
    spatial_kind, latitude_e6, longitude_e6, provider_region, adapter, source_url,
    license_identifier, attribution, provenance, created_at
ON account_data.weather_data_runs
FOR EACH ROW EXECUTE FUNCTION account_data.reject_immutable_yield_update();

CREATE TRIGGER weather_data_points_no_update
BEFORE UPDATE ON account_data.weather_data_points
FOR EACH ROW EXECUTE FUNCTION account_data.reject_immutable_yield_update();

CREATE TRIGGER yield_calculation_results_no_update
BEFORE UPDATE ON account_data.yield_calculation_results
FOR EACH ROW EXECUTE FUNCTION account_data.reject_immutable_yield_update();
