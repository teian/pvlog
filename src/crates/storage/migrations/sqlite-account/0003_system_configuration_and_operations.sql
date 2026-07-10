CREATE TABLE systems (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    timezone TEXT NOT NULL,
    commissioning_date TEXT,
    country_code TEXT,
    latitude_e6 INTEGER,
    longitude_e6 INTEGER,
    location_precision TEXT NOT NULL DEFAULT 'hidden' CHECK (location_precision IN ('hidden', 'country', 'region', 'locality', 'exact')),
    visibility TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'unlisted', 'public')),
    lifecycle TEXT NOT NULL DEFAULT 'active' CHECK (lifecycle IN ('active', 'archived', 'deleting', 'deleted')),
    status_interval_seconds INTEGER NOT NULL CHECK (status_interval_seconds BETWEEN 30 AND 86400),
    power_calculation_mode TEXT NOT NULL CHECK (power_calculation_mode IN ('reported', 'derived', 'hybrid')),
    net_calculation_mode TEXT NOT NULL CHECK (net_calculation_mode IN ('separate_flows', 'net_positive_import', 'net_positive_export')),
    calculation_settings_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(calculation_settings_json)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    archived_at INTEGER,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    CHECK (country_code IS NULL OR length(country_code) = 2),
    CHECK (latitude_e6 IS NULL OR latitude_e6 BETWEEN -90000000 AND 90000000),
    CHECK (longitude_e6 IS NULL OR longitude_e6 BETWEEN -180000000 AND 180000000),
    CHECK ((latitude_e6 IS NULL) = (longitude_e6 IS NULL)),
    CHECK (visibility = 'public' OR location_precision IN ('hidden', 'country', 'region', 'locality'))
) STRICT;

CREATE INDEX systems_lifecycle_visibility_idx
    ON systems(lifecycle, visibility, updated_at DESC);

CREATE TABLE equipment (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    equipment_kind TEXT NOT NULL CHECK (equipment_kind IN ('array', 'inverter', 'meter', 'battery', 'sensor', 'other')),
    name TEXT NOT NULL,
    manufacturer TEXT,
    model TEXT,
    serial_reference TEXT,
    capacity_watts INTEGER CHECK (capacity_watts IS NULL OR capacity_watts >= 0),
    orientation_degrees INTEGER CHECK (orientation_degrees IS NULL OR orientation_degrees BETWEEN 0 AND 359),
    tilt_degrees INTEGER CHECK (tilt_degrees IS NULL OR tilt_degrees BETWEEN 0 AND 90),
    effective_from INTEGER NOT NULL,
    effective_to INTEGER,
    configuration_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(configuration_json)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    CHECK (effective_to IS NULL OR effective_to > effective_from)
) STRICT;

CREATE INDEX equipment_system_effective_idx
    ON equipment(system_id, effective_from, effective_to);

CREATE TABLE tariffs (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    name TEXT NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('import', 'export')),
    currency_code TEXT NOT NULL CHECK (length(currency_code) = 3),
    minor_units_per_kwh INTEGER NOT NULL,
    schedule_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(schedule_json)),
    effective_from INTEGER NOT NULL,
    effective_to INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    CHECK (effective_to IS NULL OR effective_to > effective_from)
) STRICT;

CREATE INDEX tariffs_system_direction_effective_idx
    ON tariffs(system_id, direction, effective_from, effective_to);

CREATE TABLE channel_definitions (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    channel_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    data_type TEXT NOT NULL CHECK (data_type IN ('integer', 'decimal', 'boolean', 'counter')),
    unit TEXT NOT NULL,
    scale INTEGER NOT NULL CHECK (scale BETWEEN -12 AND 12),
    minimum_value INTEGER,
    maximum_value INTEGER,
    lifecycle TEXT NOT NULL DEFAULT 'active' CHECK (lifecycle IN ('active', 'retired')),
    effective_from INTEGER NOT NULL,
    effective_to INTEGER,
    display_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(display_json)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    UNIQUE (system_id, channel_key),
    CHECK (minimum_value IS NULL OR maximum_value IS NULL OR minimum_value <= maximum_value),
    CHECK (effective_to IS NULL OR effective_to > effective_from)
) STRICT;

CREATE INDEX channel_definitions_system_lifecycle_idx
    ON channel_definitions(system_id, lifecycle, effective_from);

CREATE TABLE account_audit_events (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    occurred_at INTEGER NOT NULL,
    request_id BLOB CHECK (request_id IS NULL OR length(request_id) = 16),
    actor_type TEXT NOT NULL CHECK (actor_type IN ('user', 'api_credential', 'legacy_credential', 'system', 'worker')),
    actor_id BLOB CHECK (actor_id IS NULL OR length(actor_id) = 16),
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id BLOB CHECK (target_id IS NULL OR length(target_id) = 16),
    outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'denied', 'failed')),
    previous_event_hash BLOB CHECK (previous_event_hash IS NULL OR length(previous_event_hash) = 32),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    safe_metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(safe_metadata_json)),
    UNIQUE (event_hash)
) STRICT;

CREATE INDEX account_audit_events_time_idx
    ON account_audit_events(occurred_at DESC, id);

CREATE INDEX account_audit_events_actor_idx
    ON account_audit_events(actor_type, actor_id, occurred_at DESC);

CREATE TRIGGER account_audit_events_no_update
BEFORE UPDATE ON account_audit_events
BEGIN
    SELECT RAISE(ABORT, 'account audit events are append-only');
END;

CREATE TRIGGER account_audit_events_no_delete
BEFORE DELETE ON account_audit_events
BEGIN
    SELECT RAISE(ABORT, 'account audit events are append-only');
END;

CREATE TABLE import_jobs (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    requested_by BLOB CHECK (requested_by IS NULL OR length(requested_by) = 16),
    source_identifier TEXT,
    format TEXT NOT NULL,
    dry_run INTEGER NOT NULL DEFAULT 1 CHECK (dry_run IN (0, 1)),
    state TEXT NOT NULL CHECK (state IN ('pending', 'validating', 'ready', 'running', 'completed', 'failed', 'cancelled')),
    artifact_locator TEXT,
    artifact_checksum BLOB CHECK (artifact_checksum IS NULL OR length(artifact_checksum) = 32),
    validation_report_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(validation_report_json)),
    safe_error_code TEXT,
    safe_error_detail TEXT,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    expires_at INTEGER,
    UNIQUE (source_identifier)
) STRICT;

CREATE INDEX import_jobs_state_created_idx
    ON import_jobs(state, created_at);

CREATE TABLE export_jobs (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    requested_by BLOB CHECK (requested_by IS NULL OR length(requested_by) = 16),
    system_id BLOB REFERENCES systems(id) ON DELETE SET NULL,
    format TEXT NOT NULL CHECK (format IN ('json', 'csv', 'portable_bundle')),
    state TEXT NOT NULL CHECK (state IN ('pending', 'running', 'completed', 'failed', 'cancelled', 'expired')),
    selection_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(selection_json)),
    artifact_locator TEXT,
    artifact_checksum BLOB CHECK (artifact_checksum IS NULL OR length(artifact_checksum) = 32),
    artifact_size_bytes INTEGER CHECK (artifact_size_bytes IS NULL OR artifact_size_bytes >= 0),
    safe_error_code TEXT,
    safe_error_detail TEXT,
    created_at INTEGER NOT NULL,
    started_at INTEGER,
    completed_at INTEGER,
    expires_at INTEGER
) STRICT;

CREATE INDEX export_jobs_state_created_idx
    ON export_jobs(state, created_at);

CREATE TABLE alert_rules (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    name TEXT NOT NULL,
    alert_kind TEXT NOT NULL CHECK (alert_kind IN ('idle', 'generation', 'consumption', 'net_power', 'standby_cost', 'performance', 'battery', 'extended_channel')),
    enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    condition_json TEXT NOT NULL CHECK (json_valid(condition_json)),
    schedule_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(schedule_json)),
    debounce_seconds INTEGER NOT NULL DEFAULT 0 CHECK (debounce_seconds >= 0),
    cooldown_seconds INTEGER NOT NULL DEFAULT 0 CHECK (cooldown_seconds >= 0),
    last_evaluated_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    UNIQUE (system_id, name)
) STRICT;

CREATE INDEX alert_rules_enabled_system_idx
    ON alert_rules(enabled, system_id, alert_kind);

CREATE TABLE alert_events (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    rule_id BLOB NOT NULL REFERENCES alert_rules(id) ON DELETE CASCADE CHECK (length(rule_id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    state TEXT NOT NULL CHECK (state IN ('active', 'recovered', 'acknowledged')),
    deduplication_key TEXT NOT NULL,
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    triggered_at INTEGER NOT NULL,
    recovered_at INTEGER,
    acknowledged_at INTEGER,
    UNIQUE (deduplication_key)
) STRICT;

CREATE INDEX alert_events_system_state_time_idx
    ON alert_events(system_id, state, triggered_at DESC);

CREATE TABLE webhook_subscriptions (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    name TEXT NOT NULL,
    endpoint_url TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending_verification', 'active', 'disabled', 'deleted')),
    event_types_json TEXT NOT NULL CHECK (json_valid(event_types_json)),
    encryption_key_id TEXT NOT NULL,
    encrypted_signing_secret BLOB NOT NULL,
    verification_digest BLOB CHECK (verification_digest IS NULL OR length(verification_digest) = 32),
    verified_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    UNIQUE (name),
    CHECK (endpoint_url LIKE 'https://%' OR endpoint_url LIKE 'http://%')
) STRICT;

CREATE INDEX webhook_subscriptions_state_idx
    ON webhook_subscriptions(state, updated_at);

CREATE TABLE webhook_deliveries (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    subscription_id BLOB NOT NULL REFERENCES webhook_subscriptions(id) ON DELETE CASCADE CHECK (length(subscription_id) = 16),
    event_id BLOB NOT NULL CHECK (length(event_id) = 16),
    event_type TEXT NOT NULL,
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    state TEXT NOT NULL CHECK (state IN ('pending', 'leased', 'delivered', 'retry_wait', 'dead_letter')),
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    next_attempt_at INTEGER NOT NULL,
    lease_owner TEXT,
    lease_expires_at INTEGER,
    created_at INTEGER NOT NULL,
    delivered_at INTEGER,
    UNIQUE (subscription_id, event_id)
) STRICT;

CREATE INDEX webhook_deliveries_dispatch_idx
    ON webhook_deliveries(state, next_attempt_at, lease_expires_at);

CREATE TABLE webhook_delivery_attempts (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    delivery_id BLOB NOT NULL REFERENCES webhook_deliveries(id) ON DELETE CASCADE CHECK (length(delivery_id) = 16),
    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
    started_at INTEGER NOT NULL,
    completed_at INTEGER,
    outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'retryable_failure', 'permanent_failure', 'security_failure')),
    response_status INTEGER CHECK (response_status IS NULL OR response_status BETWEEN 100 AND 599),
    safe_error_code TEXT,
    safe_response_metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(safe_response_metadata_json)),
    UNIQUE (delivery_id, attempt_number)
) STRICT;

CREATE TABLE provider_configurations (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    provider_kind TEXT NOT NULL CHECK (provider_kind IN ('insolation', 'regional_supply', 'weather', 'export')),
    name TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    endpoint_url TEXT,
    credential_secret_ref TEXT,
    configuration_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(configuration_json)),
    license_metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(license_metadata_json)),
    circuit_state TEXT NOT NULL DEFAULT 'closed' CHECK (circuit_state IN ('closed', 'open', 'half_open')),
    last_success_at INTEGER,
    last_failure_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (provider_kind, name)
) STRICT;

CREATE INDEX provider_configurations_enabled_kind_idx
    ON provider_configurations(enabled, provider_kind);

CREATE TABLE account_jobs (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    job_kind TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'leased', 'retry_wait', 'completed', 'failed', 'dead_letter', 'cancelled')),
    payload_json TEXT NOT NULL CHECK (json_valid(payload_json)),
    idempotency_key TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    max_attempts INTEGER NOT NULL CHECK (max_attempts > 0),
    available_at INTEGER NOT NULL,
    lease_owner TEXT,
    lease_expires_at INTEGER,
    last_heartbeat_at INTEGER,
    safe_error_code TEXT,
    safe_error_detail TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    completed_at INTEGER,
    UNIQUE (job_kind, idempotency_key)
) STRICT;

CREATE INDEX account_jobs_dispatch_idx
    ON account_jobs(state, priority DESC, available_at, lease_expires_at);
