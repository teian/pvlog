-- A rebuild of a system's plant-wide aggregates that spans several local years
-- runs one year per aggregation cycle. This row says where the running rebuild
-- continues, where it ends, and the time it stamps on what it writes; readings
-- received after that time are picked up once the rebuild has finished.
CREATE TABLE aggregation_progress (
    system_id BLOB PRIMARY KEY REFERENCES systems(id) ON DELETE CASCADE CHECK (length(system_id) = 16),
    next_start INTEGER NOT NULL,
    range_end INTEGER NOT NULL,
    calculated_at INTEGER NOT NULL,
    CHECK (range_end > next_start)
) STRICT;
