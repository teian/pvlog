CREATE TABLE telemetry_hot (
    observation_id BLOB PRIMARY KEY CHECK (length(observation_id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    measured_at INTEGER NOT NULL,
    received_at INTEGER NOT NULL,
    source_kind TEXT NOT NULL CHECK (source_kind IN ('modern_api', 'pvoutput_compatibility', 'import', 'provider', 'correction', 'system')),
    source_identity TEXT NOT NULL,
    idempotency_identity TEXT,
    quality_flags INTEGER NOT NULL DEFAULT 0 CHECK (quality_flags BETWEEN 0 AND 65535),
    generation_power_watts INTEGER,
    generation_energy_wh INTEGER,
    generation_lifetime_wh INTEGER,
    consumption_power_watts INTEGER,
    consumption_energy_wh INTEGER,
    consumption_lifetime_wh INTEGER,
    grid_import_power_watts INTEGER,
    grid_import_energy_wh INTEGER,
    grid_export_power_watts INTEGER,
    grid_export_energy_wh INTEGER,
    net_grid_power_watts INTEGER,
    battery_power_watts INTEGER,
    battery_energy_wh INTEGER,
    battery_state_basis_points INTEGER CHECK (battery_state_basis_points IS NULL OR battery_state_basis_points BETWEEN 0 AND 10000),
    temperature_millidegrees_c INTEGER,
    voltage_millivolts INTEGER,
    provenance_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(provenance_json)),
    canonical_hash BLOB NOT NULL CHECK (length(canonical_hash) = 32),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    UNIQUE (system_id, source_kind, source_identity, measured_at),
    UNIQUE (system_id, idempotency_identity)
) STRICT;

CREATE INDEX telemetry_hot_system_time_idx
    ON telemetry_hot(system_id, measured_at, observation_id);

CREATE INDEX telemetry_hot_received_idx
    ON telemetry_hot(received_at, system_id);

CREATE TABLE telemetry_hot_extended_values (
    observation_id BLOB NOT NULL REFERENCES telemetry_hot(observation_id) ON DELETE CASCADE CHECK (length(observation_id) = 16),
    channel_id BLOB NOT NULL REFERENCES channel_definitions(id) ON DELETE RESTRICT CHECK (length(channel_id) = 16),
    integer_value INTEGER NOT NULL,
    PRIMARY KEY (observation_id, channel_id)
) STRICT;

CREATE INDEX telemetry_hot_extended_channel_idx
    ON telemetry_hot_extended_values(channel_id, observation_id);

CREATE TABLE archived_segments (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    local_date TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK (generation > 0),
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    encoding TEXT NOT NULL CHECK (encoding IN ('protobuf_columnar')),
    compression TEXT NOT NULL CHECK (compression IN ('zstd')),
    range_start INTEGER NOT NULL,
    range_end INTEGER NOT NULL,
    point_count INTEGER NOT NULL CHECK (point_count > 0),
    field_presence BLOB NOT NULL,
    payload BLOB NOT NULL,
    compressed_length INTEGER NOT NULL CHECK (compressed_length > 0),
    uncompressed_length INTEGER NOT NULL CHECK (uncompressed_length > 0),
    content_hash BLOB NOT NULL CHECK (length(content_hash) = 32),
    state TEXT NOT NULL CHECK (state IN ('building', 'verified', 'superseded', 'corrupt')),
    created_at INTEGER NOT NULL,
    verified_at INTEGER,
    superseded_at INTEGER,
    UNIQUE (system_id, local_date, generation),
    CHECK (range_end > range_start),
    CHECK (length(payload) = compressed_length)
) STRICT;

CREATE INDEX archived_segments_system_range_idx
    ON archived_segments(system_id, range_start, range_end, state);

CREATE UNIQUE INDEX archived_segments_current_day_idx
    ON archived_segments(system_id, local_date)
    WHERE state = 'verified';

CREATE TABLE correction_overlays (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    observation_id BLOB NOT NULL CHECK (length(observation_id) = 16),
    measured_at INTEGER NOT NULL,
    segment_id BLOB REFERENCES archived_segments(id) ON DELETE SET NULL,
    operation TEXT NOT NULL CHECK (operation IN ('replace', 'delete')),
    expected_version INTEGER NOT NULL CHECK (expected_version > 0),
    replacement_json TEXT CHECK (replacement_json IS NULL OR json_valid(replacement_json)),
    reason TEXT NOT NULL,
    actor_id BLOB CHECK (actor_id IS NULL OR length(actor_id) = 16),
    request_id BLOB CHECK (request_id IS NULL OR length(request_id) = 16),
    created_at INTEGER NOT NULL,
    folded_into_generation INTEGER,
    UNIQUE (system_id, observation_id, expected_version),
    CHECK (
        (operation = 'replace' AND replacement_json IS NOT NULL)
        OR
        (operation = 'delete' AND replacement_json IS NULL)
    )
) STRICT;

CREATE INDEX correction_overlays_system_time_idx
    ON correction_overlays(system_id, measured_at, folded_into_generation);

CREATE TABLE idempotency_records (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    principal_type TEXT NOT NULL CHECK (principal_type IN ('user', 'api_credential', 'legacy_credential', 'system')),
    principal_id BLOB NOT NULL CHECK (length(principal_id) = 16),
    operation TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_hash BLOB NOT NULL CHECK (length(request_hash) = 32),
    response_status INTEGER NOT NULL CHECK (response_status BETWEEN 100 AND 599),
    response_json TEXT NOT NULL CHECK (json_valid(response_json)),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    UNIQUE (principal_type, principal_id, operation, idempotency_key),
    CHECK (expires_at > created_at)
) STRICT;

CREATE INDEX idempotency_records_expiry_idx
    ON idempotency_records(expires_at);

CREATE TABLE telemetry_rollups (
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
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
    PRIMARY KEY (system_id, resolution, bucket_start, generation),
    CHECK (bucket_end > bucket_start),
    CHECK (point_count <= expected_count OR expected_count = 0)
) STRICT;

CREATE INDEX telemetry_rollups_query_idx
    ON telemetry_rollups(system_id, resolution, bucket_start, bucket_end);

CREATE TABLE system_daily_summaries (
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    local_date TEXT NOT NULL,
    timezone TEXT NOT NULL,
    generation INTEGER NOT NULL DEFAULT 1 CHECK (generation > 0),
    generation_energy_wh INTEGER,
    consumption_energy_wh INTEGER,
    grid_import_energy_wh INTEGER,
    grid_export_energy_wh INTEGER,
    peak_generation_power_watts INTEGER,
    peak_generation_at INTEGER,
    financial_minor_units INTEGER,
    currency_code TEXT CHECK (currency_code IS NULL OR length(currency_code) = 3),
    coverage_basis_points INTEGER NOT NULL CHECK (coverage_basis_points BETWEEN 0 AND 10000),
    quality_flags INTEGER NOT NULL DEFAULT 0 CHECK (quality_flags BETWEEN 0 AND 65535),
    calculated_at INTEGER NOT NULL,
    PRIMARY KEY (system_id, local_date, generation)
) STRICT;

CREATE INDEX system_daily_summaries_date_idx
    ON system_daily_summaries(local_date, system_id);

CREATE TABLE system_lifetime_summaries (
    system_id BLOB PRIMARY KEY REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    generation INTEGER NOT NULL DEFAULT 1 CHECK (generation > 0),
    first_observation_at INTEGER,
    last_observation_at INTEGER,
    generation_energy_wh INTEGER,
    consumption_energy_wh INTEGER,
    grid_import_energy_wh INTEGER,
    grid_export_energy_wh INTEGER,
    peak_generation_power_watts INTEGER,
    peak_generation_at INTEGER,
    financial_minor_units INTEGER,
    currency_code TEXT CHECK (currency_code IS NULL OR length(currency_code) = 3),
    coverage_basis_points INTEGER NOT NULL CHECK (coverage_basis_points BETWEEN 0 AND 10000),
    calculated_at INTEGER NOT NULL,
    CHECK (first_observation_at IS NULL OR last_observation_at IS NULL OR first_observation_at <= last_observation_at)
) STRICT;

CREATE TABLE aggregation_invalidations (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    range_start INTEGER NOT NULL,
    range_end INTEGER NOT NULL,
    reason TEXT NOT NULL CHECK (reason IN ('ingestion', 'late_data', 'correction', 'deletion', 'configuration_change', 'repair')),
    required_generation INTEGER NOT NULL CHECK (required_generation > 0),
    state TEXT NOT NULL CHECK (state IN ('pending', 'leased', 'completed', 'failed')),
    lease_owner TEXT,
    lease_expires_at INTEGER,
    created_at INTEGER NOT NULL,
    completed_at INTEGER,
    UNIQUE (system_id, range_start, range_end, reason, required_generation),
    CHECK (range_end > range_start)
) STRICT;

CREATE INDEX aggregation_invalidations_dispatch_idx
    ON aggregation_invalidations(state, created_at, lease_expires_at);

CREATE TABLE data_quality_events (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    observation_id BLOB CHECK (observation_id IS NULL OR length(observation_id) = 16),
    quality_kind TEXT NOT NULL CHECK (quality_kind IN ('missing_interval', 'suspect_value', 'source_conflict', 'counter_reset', 'counter_rollover', 'rejected_ingestion', 'aggregate_lag', 'segment_corruption')),
    severity TEXT NOT NULL CHECK (severity IN ('info', 'warning', 'error', 'critical')),
    range_start INTEGER NOT NULL,
    range_end INTEGER NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('open', 'acknowledged', 'resolved')),
    details_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(details_json)),
    detected_at INTEGER NOT NULL,
    resolved_at INTEGER,
    CHECK (range_end > range_start)
) STRICT;

CREATE INDEX data_quality_events_system_state_idx
    ON data_quality_events(system_id, state, range_start, range_end);
