CREATE TABLE telemetry.partition_horizons (
    parent_table REGCLASS PRIMARY KEY,
    covered_from BIGINT NOT NULL,
    covered_until BIGINT NOT NULL,
    partition_interval TEXT NOT NULL CHECK (partition_interval = 'month'),
    updated_at BIGINT NOT NULL,
    CHECK (covered_until > covered_from)
);

CREATE OR REPLACE FUNCTION telemetry.ensure_monthly_partitions(
    p_parent_table REGCLASS,
    p_partition_prefix TEXT,
    p_covered_from TIMESTAMPTZ,
    p_covered_until TIMESTAMPTZ
) RETURNS INTEGER
LANGUAGE plpgsql
AS $$
DECLARE
    partition_start TIMESTAMPTZ;
    partition_end TIMESTAMPTZ;
    partition_name TEXT;
    created_count INTEGER := 0;
BEGIN
    IF p_covered_from >= p_covered_until THEN
        RAISE EXCEPTION 'partition horizon start must precede end';
    END IF;

    partition_start := date_trunc('month', p_covered_from AT TIME ZONE 'UTC') AT TIME ZONE 'UTC';
    WHILE partition_start < p_covered_until LOOP
        partition_end := partition_start + INTERVAL '1 month';
        partition_name := format('%s_y%sm%s', p_partition_prefix,
            to_char(partition_start, 'YYYY'), to_char(partition_start, 'MM'));

        IF to_regclass(format('telemetry.%I', partition_name)) IS NULL THEN
            EXECUTE format(
                'CREATE TABLE telemetry.%I PARTITION OF %s FOR VALUES FROM (%s) TO (%s)',
                partition_name,
                p_parent_table,
                floor(extract(epoch FROM partition_start) * 1000)::BIGINT,
                floor(extract(epoch FROM partition_end) * 1000)::BIGINT
            );
            created_count := created_count + 1;
        END IF;
        partition_start := partition_end;
    END LOOP;

    INSERT INTO telemetry.partition_horizons (
        parent_table, covered_from, covered_until, partition_interval, updated_at
    ) VALUES (
        p_parent_table,
        floor(extract(epoch FROM date_trunc('month', p_covered_from AT TIME ZONE 'UTC') AT TIME ZONE 'UTC') * 1000)::BIGINT,
        floor(extract(epoch FROM partition_start) * 1000)::BIGINT,
        'month',
        floor(extract(epoch FROM clock_timestamp()) * 1000)::BIGINT
    )
    ON CONFLICT (parent_table) DO UPDATE SET
        covered_from = LEAST(telemetry.partition_horizons.covered_from, EXCLUDED.covered_from),
        covered_until = GREATEST(telemetry.partition_horizons.covered_until, EXCLUDED.covered_until),
        updated_at = EXCLUDED.updated_at;

    RETURN created_count;
END;
$$;

ALTER TABLE telemetry.hot_extended_values
    DROP CONSTRAINT IF EXISTS hot_extended_values_account_fk,
    DROP CONSTRAINT IF EXISTS hot_extended_values_channel_fk,
    DROP CONSTRAINT IF EXISTS hot_extended_values_account_id_observation_id_fkey;
ALTER TABLE telemetry.hot_observations
    DROP CONSTRAINT IF EXISTS hot_observations_account_fk,
    DROP CONSTRAINT IF EXISTS hot_observations_system_fk;

ALTER TABLE telemetry.hot_extended_values RENAME TO hot_extended_values_unpartitioned;
ALTER TABLE telemetry.hot_observations RENAME TO hot_observations_unpartitioned;

CREATE TABLE telemetry.hot_observations_partitioned (
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
    PRIMARY KEY (account_id, observation_id, measured_at),
    UNIQUE (account_id, system_id, source_kind, source_identity, measured_at),
    FOREIGN KEY (account_id) REFERENCES management.accounts(id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
) PARTITION BY RANGE (measured_at);

SELECT telemetry.ensure_monthly_partitions(
    'telemetry.hot_observations_partitioned'::REGCLASS,
    'hot_observations',
    date_trunc('month', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' - INTERVAL '2 months',
    date_trunc('month', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' + INTERVAL '14 months'
);

INSERT INTO telemetry.hot_observations_partitioned
SELECT * FROM telemetry.hot_observations_unpartitioned;

CREATE TABLE telemetry.hot_extended_values_partitioned (
    account_id UUID NOT NULL,
    observation_id UUID NOT NULL,
    measured_at BIGINT NOT NULL,
    channel_id UUID NOT NULL,
    integer_value BIGINT NOT NULL,
    PRIMARY KEY (account_id, observation_id, measured_at, channel_id),
    FOREIGN KEY (account_id, observation_id, measured_at)
        REFERENCES telemetry.hot_observations_partitioned(account_id, observation_id, measured_at)
        ON DELETE CASCADE,
    FOREIGN KEY (account_id) REFERENCES management.accounts(id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, channel_id)
        REFERENCES account_data.channel_definitions(account_id, id) ON DELETE RESTRICT
);

INSERT INTO telemetry.hot_extended_values_partitioned (
    account_id, observation_id, measured_at, channel_id, integer_value
)
SELECT extended.account_id, extended.observation_id, observation.measured_at,
       extended.channel_id, extended.integer_value
FROM telemetry.hot_extended_values_unpartitioned AS extended
JOIN telemetry.hot_observations_unpartitioned AS observation
  ON observation.account_id = extended.account_id
 AND observation.observation_id = extended.observation_id;

DROP TABLE telemetry.hot_extended_values_unpartitioned;
DROP TABLE telemetry.hot_observations_unpartitioned;
ALTER TABLE telemetry.hot_observations_partitioned RENAME TO hot_observations;
ALTER TABLE telemetry.hot_extended_values_partitioned RENAME TO hot_extended_values;

CREATE INDEX hot_observations_system_time_partitioned_idx
    ON telemetry.hot_observations(account_id, system_id, measured_at DESC)
    INCLUDE (observation_id, quality_flags, generation_power_watts, consumption_power_watts);
CREATE INDEX hot_observations_received_partitioned_idx
    ON telemetry.hot_observations(account_id, received_at DESC, system_id);
CREATE INDEX hot_observations_measured_brin_idx
    ON telemetry.hot_observations USING BRIN(measured_at) WITH (pages_per_range = 128);
CREATE UNIQUE INDEX hot_observations_idempotency_partitioned_idx
    ON telemetry.hot_observations(account_id, system_id, idempotency_identity, measured_at)
    WHERE idempotency_identity IS NOT NULL;
CREATE INDEX hot_extended_values_channel_partitioned_idx
    ON telemetry.hot_extended_values(account_id, channel_id, measured_at DESC, observation_id);

ALTER TABLE telemetry.rollups
    DROP CONSTRAINT IF EXISTS rollups_account_fk,
    DROP CONSTRAINT IF EXISTS rollups_system_fk;
ALTER TABLE telemetry.rollups RENAME TO rollups_unpartitioned;

CREATE TABLE telemetry.rollups_partitioned (
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
    FOREIGN KEY (account_id) REFERENCES management.accounts(id) ON DELETE CASCADE,
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE,
    CHECK (bucket_end > bucket_start),
    CHECK (point_count <= expected_count OR expected_count = 0)
) PARTITION BY RANGE (bucket_start);

SELECT telemetry.ensure_monthly_partitions(
    'telemetry.rollups_partitioned'::REGCLASS,
    'rollups',
    date_trunc('month', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' - INTERVAL '25 years',
    date_trunc('month', now() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC' + INTERVAL '14 months'
);

INSERT INTO telemetry.rollups_partitioned
SELECT * FROM telemetry.rollups_unpartitioned;

DROP TABLE telemetry.rollups_unpartitioned;
ALTER TABLE telemetry.rollups_partitioned RENAME TO rollups;

CREATE INDEX rollups_query_partitioned_idx
    ON telemetry.rollups(account_id, system_id, resolution, bucket_start DESC)
    INCLUDE (bucket_end, generation, point_count, coverage_basis_points, quality_flags);
CREATE INDEX rollups_bucket_brin_idx
    ON telemetry.rollups USING BRIN(bucket_start) WITH (pages_per_range = 128);

CREATE OR REPLACE FUNCTION telemetry.ensure_partition_horizon(
    hot_until TIMESTAMPTZ,
    rollups_until TIMESTAMPTZ
) RETURNS TABLE(parent_table REGCLASS, created_partitions INTEGER, covered_until BIGINT)
LANGUAGE plpgsql
AS $$
DECLARE
    hot_created INTEGER;
    rollup_created INTEGER;
BEGIN
    hot_created := telemetry.ensure_monthly_partitions(
        'telemetry.hot_observations'::REGCLASS,
        'hot_observations',
        now() - INTERVAL '2 months',
        hot_until
    );
    rollup_created := telemetry.ensure_monthly_partitions(
        'telemetry.rollups'::REGCLASS,
        'rollups',
        now() - INTERVAL '25 years',
        rollups_until
    );

    RETURN QUERY
        SELECT horizon.parent_table, hot_created, horizon.covered_until
        FROM telemetry.partition_horizons AS horizon
        WHERE horizon.parent_table = 'telemetry.hot_observations'::REGCLASS
        UNION ALL
        SELECT horizon.parent_table, rollup_created, horizon.covered_until
        FROM telemetry.partition_horizons AS horizon
        WHERE horizon.parent_table = 'telemetry.rollups'::REGCLASS;
END;
$$;
