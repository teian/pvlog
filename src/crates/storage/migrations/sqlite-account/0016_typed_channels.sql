-- Measurement channels carry more than power: voltages, currents, counters,
-- status codes. A channel's data type, unit and scale say what its stored
-- integer means, and its aggregation kind says how a period is reduced.
--
-- data_type, unit and scale exist since the table was created, and channels
-- registered by ingestion were written as integer, scale 0, unit W, which is
-- what their values mean. The stored value stays the integer it is: at scale
-- 0 the scaled integer and the plain integer are the same number, so no value
-- is rewritten. scale counts the decimal places of the stored integer.
--
-- tracks_power and tracks_energy are superseded by the aggregation kind and
-- are no longer read. They stay so that the previous version keeps running.
ALTER TABLE channel_definitions ADD COLUMN aggregation TEXT NOT NULL DEFAULT 'mean'
    CHECK (aggregation IN ('mean', 'sum', 'min', 'max', 'last', 'counter_delta'));

UPDATE channel_definitions SET aggregation = CASE WHEN tracks_power = 1 THEN 'mean' WHEN tracks_energy = 1 THEN 'sum' ELSE 'last' END;
