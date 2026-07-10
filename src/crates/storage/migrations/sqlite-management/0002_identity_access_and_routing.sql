CREATE TABLE users (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    email TEXT NOT NULL COLLATE NOCASE,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('invited', 'active', 'disabled', 'deleted')),
    email_verified_at INTEGER,
    disabled_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    UNIQUE (email)
) STRICT;

CREATE TABLE local_credentials (
    user_id BLOB PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE CHECK (length(user_id) = 16),
    password_hash TEXT NOT NULL,
    password_changed_at INTEGER NOT NULL,
    failed_attempts INTEGER NOT NULL DEFAULT 0 CHECK (failed_attempts >= 0),
    locked_until INTEGER,
    rehash_required INTEGER NOT NULL DEFAULT 0 CHECK (rehash_required IN (0, 1))
) STRICT;

CREATE TABLE user_invitations (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    email TEXT NOT NULL COLLATE NOCASE,
    token_digest BLOB NOT NULL CHECK (length(token_digest) = 32),
    invited_by BLOB REFERENCES users(id) ON DELETE SET NULL,
    expires_at INTEGER NOT NULL,
    accepted_at INTEGER,
    revoked_at INTEGER,
    created_at INTEGER NOT NULL,
    UNIQUE (token_digest),
    CHECK (accepted_at IS NULL OR revoked_at IS NULL)
) STRICT;

CREATE INDEX user_invitations_email_idx
    ON user_invitations(email, expires_at DESC);

CREATE TABLE password_recovery_tokens (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    user_id BLOB NOT NULL REFERENCES users(id) ON DELETE CASCADE CHECK (length(user_id) = 16),
    token_digest BLOB NOT NULL CHECK (length(token_digest) = 32),
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER,
    revoked_at INTEGER,
    created_at INTEGER NOT NULL,
    UNIQUE (token_digest),
    CHECK (consumed_at IS NULL OR revoked_at IS NULL)
) STRICT;

CREATE INDEX password_recovery_tokens_user_idx
    ON password_recovery_tokens(user_id, created_at DESC);

CREATE TABLE auth_connectors (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    slug TEXT NOT NULL,
    display_name TEXT NOT NULL,
    protocol TEXT NOT NULL CHECK (protocol IN ('oidc', 'oauth2')),
    enabled INTEGER NOT NULL DEFAULT 0 CHECK (enabled IN (0, 1)),
    display_order INTEGER NOT NULL DEFAULT 0,
    discovery_url TEXT,
    issuer TEXT,
    authorization_endpoint TEXT,
    token_endpoint TEXT,
    userinfo_endpoint TEXT,
    client_id TEXT NOT NULL,
    client_secret_ref TEXT NOT NULL,
    scopes_json TEXT NOT NULL CHECK (json_valid(scopes_json)),
    claim_mapping_json TEXT NOT NULL CHECK (json_valid(claim_mapping_json)),
    pkce_required INTEGER NOT NULL DEFAULT 1 CHECK (pkce_required IN (0, 1)),
    configuration_version INTEGER NOT NULL DEFAULT 1 CHECK (configuration_version > 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (slug),
    CHECK (
        (protocol = 'oidc' AND (discovery_url IS NOT NULL OR issuer IS NOT NULL))
        OR
        (protocol = 'oauth2' AND authorization_endpoint IS NOT NULL AND token_endpoint IS NOT NULL AND userinfo_endpoint IS NOT NULL)
    )
) STRICT;

CREATE INDEX auth_connectors_enabled_order_idx
    ON auth_connectors(enabled, display_order, slug);

CREATE TABLE external_identities (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    connector_id BLOB NOT NULL REFERENCES auth_connectors(id) ON DELETE RESTRICT CHECK (length(connector_id) = 16),
    user_id BLOB NOT NULL REFERENCES users(id) ON DELETE CASCADE CHECK (length(user_id) = 16),
    provider_subject TEXT NOT NULL,
    email TEXT COLLATE NOCASE,
    email_verified INTEGER NOT NULL DEFAULT 0 CHECK (email_verified IN (0, 1)),
    display_name TEXT,
    avatar_url TEXT,
    profile_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(profile_json)),
    linked_at INTEGER NOT NULL,
    last_authenticated_at INTEGER,
    UNIQUE (connector_id, provider_subject)
) STRICT;

CREATE INDEX external_identities_user_idx
    ON external_identities(user_id, connector_id);

CREATE TABLE external_token_state (
    external_identity_id BLOB PRIMARY KEY REFERENCES external_identities(id) ON DELETE CASCADE CHECK (length(external_identity_id) = 16),
    encryption_key_id TEXT NOT NULL,
    encrypted_access_token BLOB,
    encrypted_refresh_token BLOB,
    encrypted_id_token BLOB,
    granted_scopes_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(granted_scopes_json)),
    access_expires_at INTEGER,
    refresh_expires_at INTEGER,
    updated_at INTEGER NOT NULL,
    CHECK (
        encrypted_access_token IS NOT NULL
        OR encrypted_refresh_token IS NOT NULL
        OR encrypted_id_token IS NOT NULL
    )
) STRICT;

CREATE TABLE sessions (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    user_id BLOB NOT NULL REFERENCES users(id) ON DELETE CASCADE CHECK (length(user_id) = 16),
    session_digest BLOB NOT NULL CHECK (length(session_digest) = 32),
    csrf_digest BLOB NOT NULL CHECK (length(csrf_digest) = 32),
    authentication_method TEXT NOT NULL CHECK (authentication_method IN ('local', 'oidc', 'oauth2')),
    connector_id BLOB REFERENCES auth_connectors(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    idle_expires_at INTEGER NOT NULL,
    absolute_expires_at INTEGER NOT NULL,
    rotated_at INTEGER,
    revoked_at INTEGER,
    client_metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(client_metadata_json)),
    UNIQUE (session_digest),
    CHECK (idle_expires_at <= absolute_expires_at),
    CHECK (
        (authentication_method = 'local' AND connector_id IS NULL)
        OR
        (authentication_method IN ('oidc', 'oauth2') AND connector_id IS NOT NULL)
    )
) STRICT;

CREATE INDEX sessions_user_active_idx
    ON sessions(user_id, revoked_at, absolute_expires_at);

CREATE TABLE accounts (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    slug TEXT NOT NULL,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL CHECK (status IN ('provisioning', 'active', 'suspended', 'quarantined', 'deleting', 'deleted')),
    created_by BLOB REFERENCES users(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    UNIQUE (slug)
) STRICT;

CREATE TABLE memberships (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(account_id) = 16),
    user_id BLOB NOT NULL REFERENCES users(id) ON DELETE CASCADE CHECK (length(user_id) = 16),
    status TEXT NOT NULL CHECK (status IN ('invited', 'active', 'suspended', 'revoked')),
    joined_at INTEGER,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (account_id, user_id)
) STRICT;

CREATE INDEX memberships_user_status_idx
    ON memberships(user_id, status, account_id);

CREATE TABLE api_credentials (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    owner_user_id BLOB NOT NULL REFERENCES users(id) ON DELETE CASCADE CHECK (length(owner_user_id) = 16),
    account_id BLOB REFERENCES accounts(id) ON DELETE CASCADE,
    system_id BLOB CHECK (system_id IS NULL OR length(system_id) = 16),
    name TEXT NOT NULL,
    credential_digest BLOB NOT NULL CHECK (length(credential_digest) = 32),
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    last_used_at INTEGER,
    revoked_at INTEGER,
    UNIQUE (credential_digest),
    UNIQUE (owner_user_id, name)
) STRICT;

CREATE INDEX api_credentials_account_active_idx
    ON api_credentials(account_id, revoked_at, expires_at);

CREATE TABLE api_credential_scopes (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    credential_id BLOB NOT NULL REFERENCES api_credentials(id) ON DELETE CASCADE CHECK (length(credential_id) = 16),
    scope TEXT NOT NULL,
    account_id BLOB REFERENCES accounts(id) ON DELETE CASCADE,
    system_id BLOB CHECK (system_id IS NULL OR length(system_id) = 16),
    UNIQUE (credential_id, scope, account_id, system_id)
) STRICT;

CREATE TABLE rbac_roles (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    account_id BLOB REFERENCES accounts(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    role_kind TEXT NOT NULL CHECK (role_kind IN ('built_in', 'custom')),
    built_in_key TEXT,
    created_by BLOB REFERENCES users(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0),
    UNIQUE (account_id, name),
    CHECK (
        (role_kind = 'built_in' AND built_in_key IS NOT NULL)
        OR
        (role_kind = 'custom' AND built_in_key IS NULL)
    )
) STRICT;

CREATE TABLE rbac_role_inheritance (
    role_id BLOB NOT NULL REFERENCES rbac_roles(id) ON DELETE CASCADE CHECK (length(role_id) = 16),
    parent_role_id BLOB NOT NULL REFERENCES rbac_roles(id) ON DELETE RESTRICT CHECK (length(parent_role_id) = 16),
    PRIMARY KEY (role_id, parent_role_id),
    CHECK (role_id <> parent_role_id)
) STRICT;

CREATE TABLE rbac_role_permissions (
    role_id BLOB NOT NULL REFERENCES rbac_roles(id) ON DELETE CASCADE CHECK (length(role_id) = 16),
    permission TEXT NOT NULL,
    PRIMARY KEY (role_id, permission)
) STRICT;

CREATE UNIQUE INDEX rbac_instance_role_name_idx
    ON rbac_roles(name)
    WHERE account_id IS NULL;

CREATE TABLE rbac_role_assignments (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    role_id BLOB NOT NULL REFERENCES rbac_roles(id) ON DELETE CASCADE CHECK (length(role_id) = 16),
    principal_type TEXT NOT NULL CHECK (principal_type IN ('user', 'api_credential')),
    principal_id BLOB NOT NULL CHECK (length(principal_id) = 16),
    scope_type TEXT NOT NULL CHECK (scope_type IN ('instance', 'account', 'system')),
    account_id BLOB REFERENCES accounts(id) ON DELETE CASCADE,
    system_id BLOB CHECK (system_id IS NULL OR length(system_id) = 16),
    delegated_by BLOB REFERENCES users(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER,
    revoked_at INTEGER,
    UNIQUE (role_id, principal_type, principal_id, scope_type, account_id, system_id),
    CHECK (
        (scope_type = 'instance' AND account_id IS NULL AND system_id IS NULL)
        OR
        (scope_type = 'account' AND account_id IS NOT NULL AND system_id IS NULL)
        OR
        (scope_type = 'system' AND account_id IS NOT NULL AND system_id IS NOT NULL)
    )
) STRICT;

CREATE INDEX rbac_assignments_principal_scope_idx
    ON rbac_role_assignments(principal_type, principal_id, revoked_at, expires_at);

CREATE TRIGGER rbac_assignments_principal_insert
BEFORE INSERT ON rbac_role_assignments
BEGIN
    SELECT CASE
        WHEN NEW.principal_type = 'user'
             AND NOT EXISTS (SELECT 1 FROM users WHERE id = NEW.principal_id)
            THEN RAISE(ABORT, 'RBAC user principal does not exist')
        WHEN NEW.principal_type = 'api_credential'
             AND NOT EXISTS (SELECT 1 FROM api_credentials WHERE id = NEW.principal_id)
            THEN RAISE(ABORT, 'RBAC API credential principal does not exist')
    END;
END;

CREATE TRIGGER rbac_assignments_principal_update
BEFORE UPDATE OF principal_type, principal_id ON rbac_role_assignments
BEGIN
    SELECT CASE
        WHEN NEW.principal_type = 'user'
             AND NOT EXISTS (SELECT 1 FROM users WHERE id = NEW.principal_id)
            THEN RAISE(ABORT, 'RBAC user principal does not exist')
        WHEN NEW.principal_type = 'api_credential'
             AND NOT EXISTS (SELECT 1 FROM api_credentials WHERE id = NEW.principal_id)
            THEN RAISE(ABORT, 'RBAC API credential principal does not exist')
    END;
END;

CREATE TABLE quota_policies (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    account_id BLOB REFERENCES accounts(id) ON DELETE CASCADE,
    principal_type TEXT CHECK (principal_type IN ('user', 'api_credential')),
    principal_id BLOB CHECK (principal_id IS NULL OR length(principal_id) = 16),
    systems_limit INTEGER NOT NULL CHECK (systems_limit >= 0),
    requests_per_minute INTEGER NOT NULL CHECK (requests_per_minute > 0),
    ingestion_items_per_request INTEGER NOT NULL CHECK (ingestion_items_per_request > 0),
    ingestion_items_per_minute INTEGER NOT NULL CHECK (ingestion_items_per_minute > 0),
    retained_hot_days INTEGER NOT NULL CHECK (retained_hot_days > 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CHECK (
        (principal_type IS NULL AND principal_id IS NULL)
        OR
        (principal_type IS NOT NULL AND principal_id IS NOT NULL)
    )
) STRICT;

CREATE INDEX quota_policies_resolution_idx
    ON quota_policies(account_id, principal_type, principal_id);

CREATE TABLE global_configuration (
    key TEXT PRIMARY KEY,
    value_json TEXT NOT NULL CHECK (json_valid(value_json)),
    value_class TEXT NOT NULL CHECK (value_class IN ('public', 'internal', 'secret_reference')),
    updated_by BLOB REFERENCES users(id) ON DELETE SET NULL,
    updated_at INTEGER NOT NULL,
    version INTEGER NOT NULL DEFAULT 1 CHECK (version > 0)
) STRICT;

CREATE TABLE account_database_registry (
    account_id BLOB PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(account_id) = 16),
    opaque_locator TEXT NOT NULL,
    lifecycle_state TEXT NOT NULL CHECK (lifecycle_state IN ('reserved', 'creating', 'migrating', 'verifying', 'active', 'unavailable', 'quarantined', 'deleting')),
    schema_version INTEGER NOT NULL DEFAULT 0 CHECK (schema_version >= 0),
    migration_state TEXT NOT NULL DEFAULT 'pending' CHECK (migration_state IN ('pending', 'running', 'ready', 'failed')),
    migration_owner TEXT,
    migration_lease_expires_at INTEGER,
    last_error_code TEXT,
    last_error_safe_detail TEXT,
    source_sequence INTEGER NOT NULL DEFAULT 0 CHECK (source_sequence >= 0),
    projected_sequence INTEGER NOT NULL DEFAULT 0 CHECK (projected_sequence >= 0),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    activated_at INTEGER,
    UNIQUE (opaque_locator),
    CHECK (instr(opaque_locator, '/') = 0),
    CHECK (instr(opaque_locator, '\\') = 0),
    CHECK (opaque_locator NOT IN ('.', '..')),
    CHECK (projected_sequence <= source_sequence)
) STRICT;

CREATE INDEX account_database_registry_state_idx
    ON account_database_registry(lifecycle_state, migration_state, updated_at);

CREATE TABLE account_provisioning_log (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(account_id) = 16),
    transition_from TEXT,
    transition_to TEXT NOT NULL,
    attempt INTEGER NOT NULL CHECK (attempt > 0),
    request_id BLOB CHECK (request_id IS NULL OR length(request_id) = 16),
    safe_detail_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(safe_detail_json)),
    occurred_at INTEGER NOT NULL
) STRICT;

CREATE INDEX account_provisioning_log_account_idx
    ON account_provisioning_log(account_id, occurred_at, id);

CREATE TABLE global_audit_events (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    occurred_at INTEGER NOT NULL,
    request_id BLOB CHECK (request_id IS NULL OR length(request_id) = 16),
    actor_type TEXT NOT NULL CHECK (actor_type IN ('anonymous', 'user', 'api_credential', 'system', 'worker')),
    actor_id BLOB CHECK (actor_id IS NULL OR length(actor_id) = 16),
    account_id BLOB REFERENCES accounts(id) ON DELETE SET NULL,
    action TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id BLOB CHECK (target_id IS NULL OR length(target_id) = 16),
    outcome TEXT NOT NULL CHECK (outcome IN ('succeeded', 'denied', 'failed')),
    previous_event_hash BLOB CHECK (previous_event_hash IS NULL OR length(previous_event_hash) = 32),
    event_hash BLOB NOT NULL CHECK (length(event_hash) = 32),
    safe_metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(safe_metadata_json)),
    UNIQUE (event_hash)
) STRICT;

CREATE INDEX global_audit_events_account_time_idx
    ON global_audit_events(account_id, occurred_at DESC, id);

CREATE INDEX global_audit_events_actor_time_idx
    ON global_audit_events(actor_type, actor_id, occurred_at DESC);

CREATE TRIGGER global_audit_events_no_update
BEFORE UPDATE ON global_audit_events
BEGIN
    SELECT RAISE(ABORT, 'global audit events are append-only');
END;

CREATE TRIGGER global_audit_events_no_delete
BEFORE DELETE ON global_audit_events
BEGIN
    SELECT RAISE(ABORT, 'global audit events are append-only');
END;

CREATE TABLE account_projection_checkpoints (
    account_id BLOB PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(account_id) = 16),
    source_sequence INTEGER NOT NULL CHECK (source_sequence >= 0),
    applied_sequence INTEGER NOT NULL CHECK (applied_sequence >= 0),
    projected_at INTEGER NOT NULL,
    invalidated_at INTEGER,
    CHECK (applied_sequence <= source_sequence)
) STRICT;

CREATE TABLE system_discovery_projections (
    system_id BLOB PRIMARY KEY CHECK (length(system_id) = 16),
    account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(account_id) = 16),
    display_name TEXT NOT NULL,
    country_code TEXT,
    location_label TEXT,
    location_precision TEXT NOT NULL CHECK (location_precision IN ('hidden', 'country', 'region', 'locality')),
    capacity_watts INTEGER NOT NULL CHECK (capacity_watts >= 0),
    visibility TEXT NOT NULL CHECK (visibility IN ('private', 'unlisted', 'public')),
    activity_state TEXT NOT NULL CHECK (activity_state IN ('active', 'archived', 'disabled')),
    source_sequence INTEGER NOT NULL CHECK (source_sequence > 0),
    projected_at INTEGER NOT NULL,
    invalidated_at INTEGER,
    CHECK (country_code IS NULL OR length(country_code) = 2),
    CHECK (visibility = 'public' OR location_precision = 'hidden')
) STRICT;

CREATE INDEX system_discovery_public_country_idx
    ON system_discovery_projections(visibility, country_code, activity_state, capacity_watts)
    WHERE invalidated_at IS NULL;

CREATE INDEX system_discovery_account_sequence_idx
    ON system_discovery_projections(account_id, source_sequence);
