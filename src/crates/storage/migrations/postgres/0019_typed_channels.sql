-- Measurement channels carry more than power: voltages, currents, counters,
-- status codes. A channel's data type, unit and scale say what its stored
-- integer means, and its aggregation kind says how a period is reduced.
--
-- data_type, unit and scale exist since the table was created. The stored
-- value stays the integer it is: at scale 0 the scaled integer and the plain
-- integer are the same number, so no value is rewritten. scale counts the
-- decimal places of the stored integer.
--
-- tracks_power and tracks_energy are superseded by the aggregation kind and
-- are no longer read. They stay so that the previous version keeps running.
ALTER TABLE account_data.channel_definitions
    ADD COLUMN aggregation TEXT NOT NULL DEFAULT 'mean'
        CHECK (aggregation IN ('mean', 'sum', 'min', 'max', 'last', 'counter_delta'));

UPDATE account_data.channel_definitions SET aggregation = CASE WHEN tracks_power THEN 'mean' WHEN tracks_energy THEN 'sum' ELSE 'last' END;
