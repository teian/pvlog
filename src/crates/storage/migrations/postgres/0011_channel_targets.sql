-- Channel targets bind an ingested measurement key to the equipment it meters,
-- and rollups gain the scope hierarchy the yield projections already carry.
--
-- Readings arrive under source-defined keys ("mppt-1", "wr1.mppt2"). Until an
-- operator binds a key to a string, inverter, or the plant, the key is retained
-- as 'unassigned' so its values are never silently discarded.

ALTER TABLE account_data.channel_definitions
    ADD COLUMN target_kind TEXT NOT NULL DEFAULT 'unassigned'
        CHECK (target_kind IN ('unassigned', 'plant', 'inverter', 'string')),
    ADD COLUMN target_id UUID,
    ADD COLUMN tracks_power BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN tracks_energy BOOLEAN NOT NULL DEFAULT FALSE,
    ADD COLUMN discovered_at BIGINT,
    ADD COLUMN last_value_at BIGINT,
    -- A bound channel must name its equipment; the plant target is the system itself.
    ADD CONSTRAINT channel_definitions_target_check CHECK (
        (target_kind IN ('inverter', 'string') AND target_id IS NOT NULL)
        OR (target_kind IN ('unassigned', 'plant') AND target_id IS NULL)
    );

CREATE INDEX channel_definitions_target_idx
    ON account_data.channel_definitions(account_id, system_id, target_kind, target_id);

-- The scope belongs in the primary key, otherwise a string bucket and the system
-- bucket covering the same instant would collide. Following yield_result_projections,
-- scope_id is never null and carries the system identifier for the system scope.
ALTER TABLE telemetry.rollups
    ADD COLUMN scope_kind TEXT NOT NULL DEFAULT 'system'
        CHECK (scope_kind IN ('system', 'inverter', 'string')),
    ADD COLUMN scope_id UUID;

UPDATE telemetry.rollups SET scope_id = system_id WHERE scope_id IS NULL;

ALTER TABLE telemetry.rollups
    ALTER COLUMN scope_id SET NOT NULL;

-- The table was renamed from rollups_partitioned in 0004 and constraints do not
-- follow a rename, so the primary key is dropped by lookup rather than by name.
DO $$
DECLARE
    primary_key_name TEXT;
BEGIN
    SELECT conname INTO primary_key_name
    FROM pg_constraint
    WHERE conrelid = 'telemetry.rollups'::REGCLASS AND contype = 'p';
    IF primary_key_name IS NOT NULL THEN
        EXECUTE format('ALTER TABLE telemetry.rollups DROP CONSTRAINT %I', primary_key_name);
    END IF;
END $$;

-- bucket_start stays in the key because a partitioned table requires its
-- partition key columns to be part of every unique constraint.
ALTER TABLE telemetry.rollups
    ADD PRIMARY KEY (account_id, system_id, scope_kind, scope_id, resolution, bucket_start, generation);

CREATE INDEX rollups_scope_idx
    ON telemetry.rollups(account_id, system_id, scope_kind, scope_id, resolution, bucket_start);
