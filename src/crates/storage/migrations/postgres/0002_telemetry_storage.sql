CREATE SCHEMA IF NOT EXISTS telemetry;

CREATE TABLE telemetry.hot_observations (
    account_id UUID NOT NULL,
    observation_id UUID NOT NULL,
    system_id UUID NOT NULL,
    measured_at BIGINT NOT NULL,
    received_at BIGINT NOT NULL,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('modern_api', 'pvoutput_compatibility', 'import', 'provider', 'correction', 'system')),
    source_identity TEXT NOT NULL,
    idempotency_identity TEXT,
    quality_flags INTEGER NOT NULL DEFAULT 0 CHECK (quality_flags BETWEEN 0 AND 65535),
    generation_power_watts BIGINT,
    generation_energy_wh BIGINT,
    generation_lifetime_wh BIGINT,
    consumption_power_watts BIGINT,
    consumption_energy_wh BIGINT,
    consumption_lifetime_wh BIGINT,
    grid_import_power_watts BIGINT,
    grid_import_energy_wh BIGINT,
    grid_export_power_watts BIGINT,
    grid_export_energy_wh BIGINT,
    net_grid_power_watts BIGINT,
    battery_power_watts BIGINT,
    battery_energy_wh BIGINT,
    battery_state_basis_points INTEGER CHECK (battery_state_basis_points IS NULL OR battery_state_basis_points BETWEEN 0 AND 10000),
    temperature_millidegrees_c BIGINT,
    voltage_millivolts BIGINT,
    provenance JSONB NOT NULL DEFAULT '{}'::jsonb,
    canonical_hash BYTEA NOT NULL CHECK (octet_length(canonical_hash) = 32),
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    PRIMARY KEY (account_id, observation_id),
    UNIQUE (account_id, system_id, source_kind, source_identity, measured_at)
);

CREATE INDEX hot_observations_system_time_idx
    ON telemetry.hot_observations(account_id, system_id, measured_at, observation_id);

CREATE INDEX hot_observations_received_idx
    ON telemetry.hot_observations(account_id, received_at, system_id);

CREATE UNIQUE INDEX hot_observations_idempotency_idx
    ON telemetry.hot_observations(account_id, system_id, idempotency_identity)
    WHERE idempotency_identity IS NOT NULL;

CREATE TABLE telemetry.hot_extended_values (
    account_id UUID NOT NULL,
    observation_id UUID NOT NULL,
    channel_id UUID NOT NULL,
    integer_value BIGINT NOT NULL,
    PRIMARY KEY (account_id, observation_id, channel_id),
    FOREIGN KEY (account_id, observation_id)
        REFERENCES telemetry.hot_observations(account_id, observation_id) ON DELETE CASCADE
);

CREATE INDEX hot_extended_values_channel_idx
    ON telemetry.hot_extended_values(account_id, channel_id, observation_id);

CREATE TABLE telemetry.archived_segments (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    local_date DATE NOT NULL,
    generation BIGINT NOT NULL CHECK (generation > 0),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    encoding TEXT NOT NULL CHECK (encoding = 'protobuf_columnar'),
    compression TEXT NOT NULL CHECK (compression = 'zstd'),
    range_start BIGINT NOT NULL,
    range_end BIGINT NOT NULL,
    point_count BIGINT NOT NULL CHECK (point_count > 0),
    field_presence BYTEA NOT NULL,
    payload BYTEA NOT NULL,
    compressed_length BIGINT NOT NULL CHECK (compressed_length > 0),
    uncompressed_length BIGINT NOT NULL CHECK (uncompressed_length > 0),
    content_hash BYTEA NOT NULL CHECK (octet_length(content_hash) = 32),
    state TEXT NOT NULL CHECK (state IN ('building', 'verified', 'superseded', 'corrupt')),
    created_at BIGINT NOT NULL,
    verified_at BIGINT,
    superseded_at BIGINT,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, system_id, local_date, generation),
    CHECK (range_end > range_start),
    CHECK (octet_length(payload) = compressed_length)
);

CREATE INDEX archived_segments_system_range_idx
    ON telemetry.archived_segments(account_id, system_id, range_start, range_end, state);

CREATE UNIQUE INDEX archived_segments_current_day_idx
    ON telemetry.archived_segments(account_id, system_id, local_date)
    WHERE state = 'verified';

CREATE TABLE telemetry.correction_overlays (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    observation_id UUID NOT NULL,
    measured_at BIGINT NOT NULL,
    segment_id UUID,
    operation TEXT NOT NULL CHECK (operation IN ('replace', 'delete')),
    expected_version BIGINT NOT NULL CHECK (expected_version > 0),
    replacement JSONB,
    reason TEXT NOT NULL,
    actor_id UUID,
    request_id UUID,
    created_at BIGINT NOT NULL,
    folded_into_generation BIGINT,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, system_id, observation_id, expected_version),
    FOREIGN KEY (account_id, segment_id)
        REFERENCES telemetry.archived_segments(account_id, id) ON DELETE RESTRICT,
    CHECK (
        (operation = 'replace' AND replacement IS NOT NULL)
        OR
        (operation = 'delete' AND replacement IS NULL)
    )
);

CREATE INDEX correction_overlays_system_time_idx
    ON telemetry.correction_overlays(account_id, system_id, measured_at, folded_into_generation);

CREATE TABLE telemetry.idempotency_records (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    principal_type TEXT NOT NULL CHECK (principal_type IN ('user', 'api_credential', 'legacy_credential', 'system')),
    principal_id UUID NOT NULL,
    operation TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_hash BYTEA NOT NULL CHECK (octet_length(request_hash) = 32),
    response_status INTEGER NOT NULL CHECK (response_status BETWEEN 100 AND 599),
    response JSONB NOT NULL,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, principal_type, principal_id, operation, idempotency_key),
    CHECK (expires_at > created_at)
);

CREATE INDEX idempotency_records_expiry_idx
    ON telemetry.idempotency_records(account_id, expires_at);

CREATE TABLE telemetry.rollups (
    account_id UUID NOT NULL,
    system_id UUID NOT NULL,
    resolution TEXT NOT NULL CHECK (resolution IN ('fifteen_minute', 'hour', 'day', 'month', 'year')),
    bucket_start BIGINT NOT NULL,
    bucket_end BIGINT NOT NULL,
    timezone TEXT NOT NULL,
    generation BIGINT NOT NULL DEFAULT 1 CHECK (generation > 0),
    source_generation BIGINT NOT NULL DEFAULT 1 CHECK (source_generation > 0),
    point_count BIGINT NOT NULL CHECK (point_count >= 0),
    expected_count BIGINT NOT NULL CHECK (expected_count >= 0),
    generation_energy_sum_wh BIGINT,
    generation_power_min_watts BIGINT,
    generation_power_max_watts BIGINT,
    generation_power_first_watts BIGINT,
    generation_power_last_watts BIGINT,
    consumption_energy_sum_wh BIGINT,
    grid_import_energy_sum_wh BIGINT,
    grid_export_energy_sum_wh BIGINT,
    battery_energy_delta_wh BIGINT,
    temperature_min_millidegrees_c BIGINT,
    temperature_max_millidegrees_c BIGINT,
    quality_flags INTEGER NOT NULL DEFAULT 0 CHECK (quality_flags BETWEEN 0 AND 65535),
    coverage_basis_points INTEGER NOT NULL CHECK (coverage_basis_points BETWEEN 0 AND 10000),
    calculated_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, system_id, resolution, bucket_start, generation),
    CHECK (bucket_end > bucket_start),
    CHECK (point_count <= expected_count OR expected_count = 0)
);

CREATE INDEX rollups_query_idx
    ON telemetry.rollups(account_id, system_id, resolution, bucket_start, bucket_end);

CREATE TABLE telemetry.daily_summaries (
    account_id UUID NOT NULL,
    system_id UUID NOT NULL,
    local_date DATE NOT NULL,
    timezone TEXT NOT NULL,
    generation BIGINT NOT NULL DEFAULT 1 CHECK (generation > 0),
    generation_energy_wh BIGINT,
    consumption_energy_wh BIGINT,
    grid_import_energy_wh BIGINT,
    grid_export_energy_wh BIGINT,
    peak_generation_power_watts BIGINT,
    peak_generation_at BIGINT,
    financial_minor_units BIGINT,
    currency_code TEXT CHECK (currency_code IS NULL OR length(currency_code) = 3),
    coverage_basis_points INTEGER NOT NULL CHECK (coverage_basis_points BETWEEN 0 AND 10000),
    quality_flags INTEGER NOT NULL DEFAULT 0 CHECK (quality_flags BETWEEN 0 AND 65535),
    calculated_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, system_id, local_date, generation)
);

CREATE INDEX daily_summaries_date_idx
    ON telemetry.daily_summaries(account_id, local_date, system_id);

CREATE TABLE telemetry.lifetime_summaries (
    account_id UUID NOT NULL,
    system_id UUID NOT NULL,
    generation BIGINT NOT NULL DEFAULT 1 CHECK (generation > 0),
    first_observation_at BIGINT,
    last_observation_at BIGINT,
    generation_energy_wh BIGINT,
    consumption_energy_wh BIGINT,
    grid_import_energy_wh BIGINT,
    grid_export_energy_wh BIGINT,
    peak_generation_power_watts BIGINT,
    peak_generation_at BIGINT,
    financial_minor_units BIGINT,
    currency_code TEXT CHECK (currency_code IS NULL OR length(currency_code) = 3),
    coverage_basis_points INTEGER NOT NULL CHECK (coverage_basis_points BETWEEN 0 AND 10000),
    calculated_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, system_id),
    CHECK (first_observation_at IS NULL OR last_observation_at IS NULL OR first_observation_at <= last_observation_at)
);

CREATE TABLE telemetry.aggregation_invalidations (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    range_start BIGINT NOT NULL,
    range_end BIGINT NOT NULL,
    reason TEXT NOT NULL CHECK (reason IN ('ingestion', 'late_data', 'correction', 'deletion', 'configuration_change', 'repair')),
    required_generation BIGINT NOT NULL CHECK (required_generation > 0),
    state TEXT NOT NULL CHECK (state IN ('pending', 'leased', 'completed', 'failed')),
    lease_owner TEXT,
    lease_expires_at BIGINT,
    created_at BIGINT NOT NULL,
    completed_at BIGINT,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, system_id, range_start, range_end, reason, required_generation),
    CHECK (range_end > range_start)
);

CREATE INDEX aggregation_invalidations_dispatch_idx
    ON telemetry.aggregation_invalidations(account_id, state, created_at, lease_expires_at);

CREATE TABLE telemetry.data_quality_events (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    observation_id UUID,
    quality_kind TEXT NOT NULL CHECK (quality_kind IN ('missing_interval', 'suspect_value', 'source_conflict', 'counter_reset', 'counter_rollover', 'rejected_ingestion', 'aggregate_lag', 'segment_corruption')),
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'error', 'critical')),
    range_start BIGINT NOT NULL,
    range_end BIGINT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('open', 'acknowledged', 'resolved')),
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    detected_at BIGINT NOT NULL,
    resolved_at BIGINT,
    PRIMARY KEY (account_id, id),
    CHECK (range_end > range_start)
);

CREATE INDEX data_quality_events_system_state_idx
    ON telemetry.data_quality_events(account_id, system_id, state, range_start, range_end);
