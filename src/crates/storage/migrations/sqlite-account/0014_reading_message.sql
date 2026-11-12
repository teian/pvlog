-- A reading may carry a short text from its source, such as an inverter status.
ALTER TABLE telemetry_hot ADD COLUMN message TEXT CHECK (message IS NULL OR length(message) <= 30);
