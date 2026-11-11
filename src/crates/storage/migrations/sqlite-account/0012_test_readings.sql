-- Test readings prove that a data source can deliver. They are kept apart from
-- telemetry_hot so that no total, statistic or forecast can ever include one,
-- and so that removing them cannot touch a real reading.

CREATE TABLE telemetry_test_readings (
    id BLOB PRIMARY KEY CHECK (length(id) = 16),
    system_id BLOB NOT NULL REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    data_source_id BLOB CHECK (data_source_id IS NULL OR length(data_source_id) = 16),
    measured_at INTEGER NOT NULL,
    received_at INTEGER NOT NULL,
    values_json TEXT NOT NULL CHECK (json_valid(values_json))
) STRICT;

CREATE INDEX telemetry_test_readings_system_idx
    ON telemetry_test_readings(system_id, received_at DESC);
