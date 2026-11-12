-- A reading may carry a short text from its source, such as an inverter status.
ALTER TABLE telemetry.hot_observations
    ADD COLUMN message TEXT CHECK (message IS NULL OR char_length(message) <= 30);
