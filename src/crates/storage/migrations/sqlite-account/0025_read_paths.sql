-- A channel keeps its newest value, so listing channels does not search the
-- readings for it. Values measured at the same time or later replace it.
ALTER TABLE channel_definitions ADD COLUMN last_value INTEGER;
ALTER TABLE channel_definitions ADD COLUMN last_value_measured_at INTEGER;

UPDATE channel_definitions SET (last_value, last_value_measured_at) = (
    SELECT v.integer_value, h.measured_at
    FROM telemetry_hot h INDEXED BY telemetry_hot_system_time_idx
    CROSS JOIN telemetry_hot_extended_values v
        ON v.observation_id = h.observation_id AND v.channel_id = channel_definitions.id
    WHERE h.system_id = channel_definitions.system_id
    ORDER BY h.measured_at DESC
    LIMIT 1
);

-- The newest reading a system or a data source received.
CREATE INDEX telemetry_hot_system_received_idx
    ON telemetry_hot(system_id, received_at, measured_at);
CREATE INDEX telemetry_hot_data_source_received_idx
    ON telemetry_hot(data_source_id, received_at) WHERE data_source_id IS NOT NULL;

-- How far a system's aggregates reach.
CREATE INDEX telemetry_rollups_system_end_idx ON telemetry_rollups(system_id, bucket_end);
