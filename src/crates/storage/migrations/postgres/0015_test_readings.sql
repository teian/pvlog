-- Test readings prove that a data source can deliver. They are kept apart from
-- the hot observations so that no total, statistic or forecast can ever include
-- one, and so that removing them cannot touch a real reading.

CREATE TABLE telemetry.test_readings (
    account_id UUID NOT NULL,
    id UUID NOT NULL,
    system_id UUID NOT NULL,
    data_source_id UUID,
    measured_at BIGINT NOT NULL,
    received_at BIGINT NOT NULL,
    reading_values JSONB NOT NULL,
    PRIMARY KEY (account_id, id),
    FOREIGN KEY (account_id, system_id)
        REFERENCES account_data.systems(account_id, id) ON DELETE CASCADE
);

CREATE INDEX test_readings_system_idx
    ON telemetry.test_readings(account_id, system_id, received_at DESC);
