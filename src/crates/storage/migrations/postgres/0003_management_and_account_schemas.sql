CREATE SCHEMA IF NOT EXISTS management;
CREATE SCHEMA IF NOT EXISTS account_data;
CREATE SCHEMA IF NOT EXISTS community;
CREATE SCHEMA IF NOT EXISTS integrations;
CREATE SCHEMA IF NOT EXISTS jobs;

CREATE TABLE management.users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('invited', 'active', 'disabled', 'deleted')),
    email_verified_at BIGINT,
    disabled_at BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    UNIQUE (email)
);

CREATE UNIQUE INDEX users_email_casefold_idx ON management.users(lower(email));

CREATE TABLE management.local_credentials (
    user_id UUID PRIMARY KEY REFERENCES management.users(id) ON DELETE CASCADE,
    password_hash TEXT NOT NULL,
    password_changed_at BIGINT NOT NULL,
    failed_attempts INTEGER NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0),
    locked_until BIGINT,
    rehash_required BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE management.user_invitations (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL,
    token_digest BYTEA NOT NULL CHECK (octet_length(token_digest) = 32),
    invited_by UUID REFERENCES management.users(id) ON DELETE SET NULL,
    expires_at BIGINT NOT NULL,
    accepted_at BIGINT,
    revoked_at BIGINT,
    created_at BIGINT NOT NULL,
    UNIQUE (token_digest),
    CHECK (accepted_at IS NULL OR revoked_at IS NULL)
);

CREATE INDEX user_invitations_email_idx
    ON management.user_invitations(lower(email), expires_at DESC);

CREATE TABLE management.password_recovery_tokens (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES management.users(id) ON DELETE CASCADE,
    token_digest BYTEA NOT NULL CHECK (octet_length(token_digest) = 32),
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT,
    revoked_at BIGINT,
    created_at BIGINT NOT NULL,
    UNIQUE (token_digest),
    CHECK (consumed_at IS NULL OR revoked_at IS NULL)
);

CREATE INDEX password_recovery_tokens_user_idx
    ON management.password_recovery_tokens(user_id, created_at DESC);

CREATE TABLE management.auth_connectors (
    id UUID PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    protocol TEXT NOT NULL CHECK (protocol IN ('oidc', 'oauth2')),
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    display_order INTEGER NOT NULL DEFAULT 0,
    discovery_url TEXT,
    issuer TEXT,
    authorization_endpoint TEXT,
    token_endpoint TEXT,
    userinfo_endpoint TEXT,
    client_id TEXT NOT NULL,
    client_secret_ref TEXT NOT NULL,
    scopes JSONB NOT NULL,
    claim_mapping JSONB NOT NULL,
    pkce_required BOOLEAN NOT NULL DEFAULT TRUE,
    configuration_version BIGINT NOT NULL DEFAULT 1 CHECK (configuration_version > 0),
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CHECK (
        (protocol = 'oidc' AND (discovery_url IS NOT NULL OR issuer IS NOT NULL))
        OR
        (protocol = 'oauth2' AND authorization_endpoint IS NOT NULL AND token_endpoint IS NOT NULL AND userinfo_endpoint IS NOT NULL)
    )
);

CREATE INDEX auth_connectors_enabled_order_idx
    ON management.auth_connectors(enabled, display_order, slug);

CREATE TABLE management.external_identities (
    id UUID PRIMARY KEY,
    connector_id UUID NOT NULL REFERENCES management.auth_connectors(id) ON DELETE RESTRICT,
    user_id UUID NOT NULL REFERENCES management.users(id) ON DELETE CASCADE,
    provider_subject TEXT NOT NULL,
    email TEXT,
    email_verified BOOLEAN NOT NULL DEFAULT FALSE,
    display_name TEXT,
    avatar_url TEXT,
    profile JSONB NOT NULL DEFAULT '{}'::jsonb,
    linked_at BIGINT NOT NULL,
    last_authenticated_at BIGINT,
    UNIQUE (connector_id, provider_subject)
);

CREATE INDEX external_identities_user_idx
    ON management.external_identities(user_id, connector_id);

CREATE TABLE management.external_token_state (
    external_identity_id UUID PRIMARY KEY REFERENCES management.external_identities(id) ON DELETE CASCADE,
    encryption_key_id TEXT NOT NULL,
    encrypted_access_token BYTEA,
    encrypted_refresh_token BYTEA,
    encrypted_id_token BYTEA,
    granted_scopes JSONB NOT NULL DEFAULT '[]'::jsonb,
    access_expires_at BIGINT,
    refresh_expires_at BIGINT,
    updated_at BIGINT NOT NULL,
    CHECK (
        encrypted_access_token IS NOT NULL
        OR encrypted_refresh_token IS NOT NULL
        OR encrypted_id_token IS NOT NULL
    )
);

CREATE TABLE management.sessions (
    id UUID PRIMARY KEY,
    user_id UUID NOT NULL REFERENCES management.users(id) ON DELETE CASCADE,
    session_digest BYTEA NOT NULL CHECK (octet_length(session_digest) = 32),
    csrf_digest BYTEA NOT NULL CHECK (octet_length(csrf_digest) = 32),
    authentication_method TEXT NOT NULL CHECK (authentication_method IN ('local', 'oidc', 'oauth2')),
    connector_id UUID REFERENCES management.auth_connectors(id) ON DELETE SET NULL,
    created_at BIGINT NOT NULL,
    last_seen_at BIGINT NOT NULL,
    idle_expires_at BIGINT NOT NULL,
    absolute_expires_at BIGINT NOT NULL,
    rotated_at BIGINT,
    revoked_at BIGINT,
    client_metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (session_digest),
    CHECK (idle_expires_at <= absolute_expires_at),
    CHECK (
        (authentication_method = 'local' AND connector_id IS NULL)
        OR
        (authentication_method IN ('oidc', 'oauth2') AND connector_id IS NOT NULL)
    )
);

CREATE INDEX sessions_user_active_idx
    ON management.sessions(user_id, revoked_at, absolute_expires_at);

CREATE TABLE management.accounts (
    id UUID PRIMARY KEY,
    slug TEXT NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('provisioning', 'active', 'suspended', 'quarantined', 'deleting', 'deleted')),
    created_by UUID REFERENCES management.users(id) ON DELETE SET NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0)
);

CREATE TABLE management.memberships (
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES management.users(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('invited', 'active', 'suspended', 'revoked')),
    joined_at BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, user_id)
);

CREATE INDEX memberships_user_status_idx
    ON management.memberships(user_id, status, account_id);

CREATE TABLE management.api_credentials (
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    owner_user_id UUID NOT NULL REFERENCES management.users(id) ON DELETE CASCADE,
    system_id UUID,
    name TEXT NOT NULL,
    credential_digest BYTEA NOT NULL CHECK (octet_length(credential_digest) = 32),
    created_at BIGINT NOT NULL,
    expires_at BIGINT,
    last_used_at BIGINT,
    revoked_at BIGINT,
    PRIMARY KEY (account_id, id),
    UNIQUE (credential_digest),
    UNIQUE (account_id, owner_user_id, name)
);

CREATE TABLE management.api_credential_scopes (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    credential_id UUID NOT NULL,
    scope TEXT NOT NULL,
    system_id UUID,
    PRIMARY KEY (account_id, id),
    UNIQUE NULLS NOT DISTINCT (account_id, credential_id, scope, system_id),
    FOREIGN KEY (account_id, credential_id)
        REFERENCES management.api_credentials(account_id, id) ON DELETE CASCADE
);

CREATE TABLE management.rbac_roles (
    account_id UUID REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    name TEXT NOT NULL,
    role_kind TEXT NOT NULL CHECK (role_kind IN ('built_in', 'custom')),
    built_in_key TEXT,
    created_by UUID REFERENCES management.users(id) ON DELETE SET NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    UNIQUE NULLS NOT DISTINCT (account_id, id),
    UNIQUE NULLS NOT DISTINCT (account_id, name),
    CHECK (
        (role_kind = 'built_in' AND built_in_key IS NOT NULL)
        OR
        (role_kind = 'custom' AND built_in_key IS NULL)
    )
);

CREATE TABLE management.rbac_role_inheritance (
    account_id UUID,
    role_id UUID NOT NULL,
    parent_role_id UUID NOT NULL,
    PRIMARY KEY (account_id, role_id, parent_role_id),
    FOREIGN KEY (account_id, role_id) REFERENCES management.rbac_roles(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, parent_role_id) REFERENCES management.rbac_roles(account_id, id) ON DELETE RESTRICT,
    CHECK (role_id <> parent_role_id)
);

CREATE TABLE management.rbac_role_permissions (
    account_id UUID,
    role_id UUID NOT NULL,
    permission TEXT NOT NULL,
    PRIMARY KEY (account_id, role_id, permission),
    FOREIGN KEY (account_id, role_id) REFERENCES management.rbac_roles(account_id, id) ON DELETE CASCADE
);

CREATE TABLE management.rbac_role_assignments (
    account_id UUID REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    role_id UUID NOT NULL,
    principal_type TEXT NOT NULL CHECK (principal_type IN ('user', 'api_credential')),
    principal_id UUID NOT NULL,
    scope_type TEXT NOT NULL CHECK (scope_type IN ('instance', 'account', 'system')),
    system_id UUID,
    delegated_by UUID REFERENCES management.users(id) ON DELETE SET NULL,
    created_at BIGINT NOT NULL,
    expires_at BIGINT,
    revoked_at BIGINT,
    UNIQUE NULLS NOT DISTINCT (account_id, id),
    UNIQUE NULLS NOT DISTINCT (account_id, role_id, principal_type, principal_id, scope_type, system_id),
    FOREIGN KEY (account_id, role_id) REFERENCES management.rbac_roles(account_id, id) ON DELETE CASCADE,
    CHECK (
        (scope_type = 'instance' AND account_id IS NULL AND system_id IS NULL)
        OR
        (scope_type = 'account' AND account_id IS NOT NULL AND system_id IS NULL)
        OR
        (scope_type = 'system' AND account_id IS NOT NULL AND system_id IS NOT NULL)
    )
);

CREATE INDEX rbac_assignments_principal_idx
    ON management.rbac_role_assignments(principal_type, principal_id, revoked_at, expires_at);

CREATE TABLE management.quota_policies (
    account_id UUID REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    principal_type TEXT CHECK (principal_type IN ('user', 'api_credential')),
    principal_id UUID,
    systems_limit BIGINT NOT NULL CHECK (systems_limit >= 0),
    requests_per_minute BIGINT NOT NULL CHECK (requests_per_minute > 0),
    ingestion_items_per_request BIGINT NOT NULL CHECK (ingestion_items_per_request > 0),
    ingestion_items_per_minute BIGINT NOT NULL CHECK (ingestion_items_per_minute > 0),
    retained_hot_days BIGINT NOT NULL CHECK (retained_hot_days > 0),
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    UNIQUE NULLS NOT DISTINCT (account_id, id),
    CHECK ((principal_type IS NULL) = (principal_id IS NULL))
);

CREATE TABLE management.global_configuration (
    key TEXT PRIMARY KEY,
    value JSONB NOT NULL,
    value_class TEXT NOT NULL CHECK (value_class IN ('public', 'internal', 'secret_reference')),
    updated_by UUID REFERENCES management.users(id) ON DELETE SET NULL,
    updated_at BIGINT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0)
);

CREATE TABLE management.account_storage_registry (
    account_id UUID PRIMARY KEY REFERENCES management.accounts(id) ON DELETE CASCADE,
    storage_kind TEXT NOT NULL CHECK (storage_kind = 'postgres'),
    schema_version BIGINT NOT NULL CHECK (schema_version >= 0),
    migration_state TEXT NOT NULL CHECK (migration_state IN ('pending', 'running', 'ready', 'failed')),
    last_error_code TEXT,
    last_error_safe_detail TEXT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL
);

CREATE TABLE management.global_audit_events (
    id UUID PRIMARY KEY,
    occurred_at BIGINT NOT NULL,
    request_id UUID,
    actor_type TEXT NOT NULL CHECK (actor_type IN ('anonymous', 'user', 'api_credential', 'system', 'worker')),
    actor_id UUID,
    account_id UUID REFERENCES management.accounts(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id UUID,
    outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'denied', 'failed')),
    previous_event_hash BYTEA CHECK (previous_event_hash IS NULL OR octet_length(previous_event_hash) = 32),
    event_hash BYTEA NOT NULL CHECK (octet_length(event_hash) = 32),
    safe_metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    UNIQUE (event_hash)
);

CREATE INDEX global_audit_events_account_time_idx
    ON management.global_audit_events(account_id, occurred_at DESC, id);

CREATE FUNCTION management.reject_audit_mutation() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    RAISE EXCEPTION 'audit events are append-only';
END;
$$;

CREATE TRIGGER global_audit_events_no_mutation
BEFORE UPDATE OR DELETE ON management.global_audit_events
FOR EACH ROW EXECUTE FUNCTION management.reject_audit_mutation();

CREATE TABLE account_data.systems (
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    timezone TEXT NOT NULL,
    commissioning_date DATE,
    country_code TEXT CHECK (country_code IS NULL OR length(country_code) = 2),
    latitude_e6 INTEGER CHECK (latitude_e6 IS NULL OR latitude_e6 BETWEEN -90000000 AND 90000000),
    longitude_e6 INTEGER CHECK (longitude_e6 IS NULL OR longitude_e6 BETWEEN -180000000 AND 180000000),
    location_precision TEXT NOT NULL DEFAULT 'hidden' CHECK (location_precision IN ('hidden', 'country', 'region', 'locality', 'exact')),
    visibility TEXT NOT NULL DEFAULT 'private' CHECK (visibility IN ('private', 'unlisted', 'public')),
    lifecycle TEXT NOT NULL DEFAULT 'active' CHECK (lifecycle IN ('active', 'archived', 'deleting', 'deleted')),
    status_interval_seconds INTEGER NOT NULL CHECK (status_interval_seconds BETWEEN 30 AND 86400),
    power_calculation_mode TEXT NOT NULL CHECK (power_calculation_mode IN ('reported', 'derived', 'hybrid')),
    net_calculation_mode TEXT NOT NULL CHECK (net_calculation_mode IN ('separate_flows', 'net_positive_import', 'net_positive_export')),
    calculation_settings JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    archived_at BIGINT,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    PRIMARY KEY (account_id, id),
    CHECK ((latitude_e6 IS NULL) = (longitude_e6 IS NULL))
);

CREATE INDEX systems_lifecycle_visibility_idx
    ON account_data.systems(account_id, lifecycle, visibility, updated_at DESC);

CREATE TABLE account_data.equipment (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    equipment_kind TEXT NOT NULL CHECK (equipment_kind IN ('array', 'inverter', 'meter', 'battery', 'sensor', 'other')),
    name TEXT NOT NULL,
    capacity_watts BIGINT CHECK (capacity_watts IS NULL OR capacity_watts >= 0),
    effective_from BIGINT NOT NULL,
    effective_to BIGINT,
    configuration JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    PRIMARY KEY (account_id, id),
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    CHECK (effective_to IS NULL OR effective_to > effective_from)
);

CREATE INDEX equipment_system_effective_idx
    ON account_data.equipment(account_id, system_id, effective_from, effective_to);

CREATE TABLE account_data.tariffs (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    name TEXT NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('import', 'export')),
    currency_code TEXT NOT NULL CHECK (length(currency_code) = 3),
    minor_units_per_kwh BIGINT NOT NULL,
    schedule JSONB NOT NULL DEFAULT '{}'::jsonb,
    effective_from BIGINT NOT NULL,
    effective_to BIGINT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    PRIMARY KEY (account_id, id),
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    CHECK (effective_to IS NULL OR effective_to > effective_from)
);

CREATE TABLE account_data.channel_definitions (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    channel_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    data_type TEXT NOT NULL CHECK (data_type IN ('integer', 'decimal', 'boolean', 'counter')),
    unit TEXT NOT NULL,
    scale INTEGER NOT NULL CHECK (scale BETWEEN -12 AND 12),
    minimum_value BIGINT,
    maximum_value BIGINT,
    lifecycle TEXT NOT NULL DEFAULT 'active' CHECK (lifecycle IN ('active', 'retired')),
    effective_from BIGINT NOT NULL,
    effective_to BIGINT,
    display JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, system_id, channel_key),
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    CHECK (minimum_value IS NULL OR maximum_value IS NULL OR minimum_value <= maximum_value),
    CHECK (effective_to IS NULL OR effective_to > effective_from)
);

CREATE TABLE account_data.audit_events (
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    occurred_at BIGINT NOT NULL,
    request_id UUID,
    actor_type TEXT NOT NULL,
    actor_id UUID,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id UUID,
    outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'denied', 'failed')),
    previous_event_hash BYTEA CHECK (previous_event_hash IS NULL OR octet_length(previous_event_hash) = 32),
    event_hash BYTEA NOT NULL CHECK (octet_length(event_hash) = 32),
    safe_metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, event_hash)
);

CREATE TRIGGER account_audit_events_no_mutation
BEFORE UPDATE OR DELETE ON account_data.audit_events
FOR EACH ROW EXECUTE FUNCTION management.reject_audit_mutation();

CREATE TABLE account_data.imports (
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    requested_by UUID,
    source_identifier TEXT,
    format TEXT NOT NULL,
    dry_run BOOLEAN NOT NULL DEFAULT TRUE,
    state TEXT NOT NULL CHECK (state IN ('pending', 'validating', 'ready', 'running', 'completed', 'failed', 'cancelled')),
    artifact_locator TEXT,
    artifact_checksum BYTEA CHECK (artifact_checksum IS NULL OR octet_length(artifact_checksum) = 32),
    validation_report JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at BIGINT NOT NULL,
    completed_at BIGINT,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, source_identifier)
);

CREATE TABLE account_data.exports (
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    requested_by UUID,
    system_id UUID,
    format TEXT NOT NULL CHECK (format IN ('json', 'csv', 'portable_bundle')),
    state TEXT NOT NULL CHECK (state IN ('pending', 'running', 'completed', 'failed', 'cancelled', 'expired')),
    selection JSONB NOT NULL DEFAULT '{}'::jsonb,
    artifact_locator TEXT,
    artifact_checksum BYTEA CHECK (artifact_checksum IS NULL OR octet_length(artifact_checksum) = 32),
    created_at BIGINT NOT NULL,
    completed_at BIGINT,
    expires_at BIGINT,
    PRIMARY KEY (account_id, id),
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE SET NULL (system_id)
);

CREATE TABLE account_data.alert_rules (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    name TEXT NOT NULL,
    alert_kind TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    condition JSONB NOT NULL,
    schedule JSONB NOT NULL DEFAULT '{}'::jsonb,
    debounce_seconds BIGINT NOT NULL DEFAULT 0 CHECK (debounce_seconds >= 0),
    cooldown_seconds BIGINT NOT NULL DEFAULT 0 CHECK (cooldown_seconds >= 0),
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    version BIGINT NOT NULL DEFAULT 1 CHECK (version > 0),
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, system_id, name),
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);

CREATE TABLE account_data.alert_events (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    rule_id UUID NOT NULL,
    system_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('active', 'recovered', 'acknowledged')),
    deduplication_key TEXT NOT NULL,
    payload JSONB NOT NULL,
    triggered_at BIGINT NOT NULL,
    recovered_at BIGINT,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, deduplication_key),
    FOREIGN KEY (account_id, rule_id) REFERENCES account_data.alert_rules(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);

CREATE TABLE community.teams (
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    name TEXT NOT NULL,
    visibility TEXT NOT NULL CHECK (visibility IN ('private', 'unlisted', 'public')),
    owner_user_id UUID NOT NULL REFERENCES management.users(id) ON DELETE RESTRICT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, name)
);

CREATE TABLE community.team_memberships (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    team_id UUID NOT NULL,
    system_account_id UUID NOT NULL,
    system_id UUID NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('pending', 'active', 'left', 'removed')),
    joined_at BIGINT,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, team_id, system_account_id, system_id),
    FOREIGN KEY (account_id, team_id) REFERENCES community.teams(account_id, id) ON DELETE CASCADE,
    FOREIGN KEY (system_account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);

CREATE TABLE community.favourites (
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    user_id UUID NOT NULL REFERENCES management.users(id) ON DELETE CASCADE,
    system_account_id UUID NOT NULL,
    system_id UUID NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, user_id, system_account_id, system_id),
    FOREIGN KEY (system_account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);

CREATE TABLE community.system_projections (
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    system_id UUID NOT NULL,
    display_name TEXT NOT NULL,
    country_code TEXT,
    location_precision TEXT NOT NULL,
    capacity_watts BIGINT NOT NULL CHECK (capacity_watts >= 0),
    visibility TEXT NOT NULL CHECK (visibility IN ('private', 'unlisted', 'public')),
    activity_state TEXT NOT NULL,
    source_sequence BIGINT NOT NULL CHECK (source_sequence > 0),
    projected_at BIGINT NOT NULL,
    invalidated_at BIGINT,
    PRIMARY KEY (account_id, system_id),
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);

CREATE INDEX system_projections_discovery_idx
    ON community.system_projections(visibility, country_code, activity_state, capacity_watts)
    WHERE invalidated_at IS NULL;

CREATE TABLE community.team_rollup_projections (
    account_id UUID NOT NULL,
    team_id UUID NOT NULL,
    period_start BIGINT NOT NULL,
    period_end BIGINT NOT NULL,
    generation_energy_wh BIGINT NOT NULL,
    normalized_generation_wh_per_kw BIGINT,
    coverage_basis_points INTEGER NOT NULL CHECK (coverage_basis_points BETWEEN 0 AND 10000),
    source_sequence BIGINT NOT NULL,
    projected_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, team_id, period_start),
    FOREIGN KEY (account_id, team_id) REFERENCES community.teams(account_id, id) ON DELETE CASCADE,
    CHECK (period_end > period_start)
);

CREATE TABLE integrations.webhook_subscriptions (
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    name TEXT NOT NULL,
    endpoint_url TEXT NOT NULL,
    state TEXT NOT NULL,
    event_types JSONB NOT NULL,
    encryption_key_id TEXT NOT NULL,
    encrypted_signing_secret BYTEA NOT NULL,
    verification_digest BYTEA CHECK (verification_digest IS NULL OR octet_length(verification_digest) = 32),
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, name)
);

CREATE TABLE integrations.webhook_deliveries (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    subscription_id UUID NOT NULL,
    event_id UUID NOT NULL,
    event_type TEXT NOT NULL,
    schema_version INTEGER NOT NULL CHECK (schema_version > 0),
    payload JSONB NOT NULL,
    state TEXT NOT NULL,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    next_attempt_at BIGINT NOT NULL,
    lease_owner TEXT,
    lease_expires_at BIGINT,
    created_at BIGINT NOT NULL,
    delivered_at BIGINT,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, subscription_id, event_id),
    FOREIGN KEY (account_id, subscription_id)
        REFERENCES integrations.webhook_subscriptions(account_id, id) ON DELETE CASCADE
);

CREATE INDEX webhook_deliveries_dispatch_idx
    ON integrations.webhook_deliveries(account_id, state, next_attempt_at, lease_expires_at);

CREATE TABLE integrations.webhook_delivery_attempts (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    delivery_id UUID NOT NULL,
    attempt_number INTEGER NOT NULL CHECK (attempt_number > 0),
    started_at BIGINT NOT NULL,
    completed_at BIGINT,
    outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'retryable_failure', 'permanent_failure', 'security_failure')),
    response_status INTEGER CHECK (response_status IS NULL OR response_status BETWEEN 100 AND 599),
    safe_error_code TEXT,
    safe_response_metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, delivery_id, attempt_number),
    FOREIGN KEY (account_id, delivery_id)
        REFERENCES integrations.webhook_deliveries(account_id, id) ON DELETE CASCADE
);

CREATE TABLE integrations.providers (
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    provider_kind TEXT NOT NULL CHECK (provider_kind IN ('insolation', 'regional_supply', 'weather', 'export')),
    name TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    endpoint_url TEXT,
    credential_secret_ref TEXT,
    configuration JSONB NOT NULL DEFAULT '{}'::jsonb,
    license_metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    circuit_state TEXT NOT NULL DEFAULT 'closed',
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, provider_kind, name)
);

CREATE TABLE jobs.account_jobs (
    account_id UUID NOT NULL REFERENCES management.accounts(id) ON DELETE CASCADE,
    id UUID NOT NULL,
    job_kind TEXT NOT NULL,
    state TEXT NOT NULL,
    payload JSONB NOT NULL,
    idempotency_key TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
    max_attempts INTEGER NOT NULL CHECK (max_attempts > 0),
    available_at BIGINT NOT NULL,
    lease_owner TEXT,
    lease_expires_at BIGINT,
    last_heartbeat_at BIGINT,
    safe_error_code TEXT,
    safe_error_detail TEXT,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    completed_at BIGINT,
    PRIMARY KEY (account_id, id),
    UNIQUE (account_id, job_kind, idempotency_key)
);

CREATE INDEX account_jobs_dispatch_idx
    ON jobs.account_jobs(account_id, state, priority DESC, available_at, lease_expires_at);

ALTER TABLE telemetry.hot_observations
    ADD CONSTRAINT hot_observations_system_fk
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE;

ALTER TABLE telemetry.hot_extended_values
    ADD CONSTRAINT hot_extended_values_channel_fk
    FOREIGN KEY (account_id, channel_id) REFERENCES account_data.channel_definitions(account_id, id) ON DELETE RESTRICT;

ALTER TABLE telemetry.archived_segments
    ADD CONSTRAINT archived_segments_system_fk
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE;

ALTER TABLE telemetry.correction_overlays
    ADD CONSTRAINT correction_overlays_system_fk
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE;

ALTER TABLE telemetry.idempotency_records
    ADD CONSTRAINT idempotency_records_account_fk
    FOREIGN KEY (account_id) REFERENCES management.accounts(id) ON DELETE CASCADE;

ALTER TABLE telemetry.rollups
    ADD CONSTRAINT rollups_system_fk
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE;

ALTER TABLE telemetry.daily_summaries
    ADD CONSTRAINT daily_summaries_system_fk
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE;

ALTER TABLE telemetry.lifetime_summaries
    ADD CONSTRAINT lifetime_summaries_system_fk
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE;

ALTER TABLE telemetry.aggregation_invalidations
    ADD CONSTRAINT aggregation_invalidations_system_fk
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE;

ALTER TABLE telemetry.data_quality_events
    ADD CONSTRAINT data_quality_events_system_fk
    FOREIGN KEY (account_id, system_id) REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE;
