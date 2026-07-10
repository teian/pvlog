CREATE TABLE teams (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(account_id) = 16),
    name TEXT NOT NULL,
    visibility TEXT NOT NULL CHECK (visibility IN ('private', 'unlisted', 'public')),
    owner_user_id BLOB NOT NULL REFERENCES users(id) ON DELETE RESTRICT CHECK (length(owner_user_id) = 16),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    UNIQUE (account_id, name)
) STRICT;

CREATE TABLE team_memberships (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    team_account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(team_account_id) = 16),
    team_id BLOB NOT NULL REFERENCES teams(id) ON DELETE CASCADE CHECK (length(team_id) = 16),
    system_account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(system_account_id) = 16),
    system_id BLOB NOT NULL CHECK (length(system_id) = 16),
    state TEXT NOT NULL CHECK (state IN ('pending', 'active', 'left', 'removed')),
    joined_at INTEGER,
    UNIQUE (team_account_id, team_id, system_account_id, system_id)
) STRICT;

CREATE INDEX team_memberships_system_idx
    ON team_memberships(system_account_id, system_id, state);

CREATE TABLE favourites (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(account_id) = 16),
    user_id BLOB NOT NULL REFERENCES users(id) ON DELETE CASCADE CHECK (length(user_id) = 16),
    system_account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(system_account_id) = 16),
    system_id BLOB NOT NULL CHECK (length(system_id) = 16),
    created_at INTEGER NOT NULL,
    UNIQUE (account_id, user_id, system_account_id, system_id)
) STRICT;

CREATE TABLE team_rollup_projections (
    team_account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE CHECK (length(team_account_id) = 16),
    team_id BLOB NOT NULL REFERENCES teams(id) ON DELETE CASCADE CHECK (length(team_id) = 16),
    period_start INTEGER NOT NULL,
    period_end INTEGER NOT NULL,
    generation_energy_wh INTEGER NOT NULL,
    normalized_generation_wh_per_kw INTEGER,
    coverage_basis_points INTEGER NOT NULL CHECK (coverage_basis_points BETWEEN 0 AND 10000),
    source_sequence INTEGER NOT NULL CHECK (source_sequence > 0),
    projected_at INTEGER NOT NULL,
    PRIMARY KEY (team_account_id, team_id, period_start),
    CHECK (period_end > period_start)
) STRICT;

CREATE INDEX team_rollup_projections_period_idx
    ON team_rollup_projections(team_id, period_start, period_end);
