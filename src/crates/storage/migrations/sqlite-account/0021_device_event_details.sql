-- What a device says about an event beyond its code and message: its type,
-- severity category and group, the value a changed setting had before and has
-- now, and the user group that caused the change.
ALTER TABLE device_events ADD COLUMN event_type TEXT;
ALTER TABLE device_events ADD COLUMN category TEXT;
ALTER TABLE device_events ADD COLUMN event_group TEXT;
ALTER TABLE device_events ADD COLUMN old_value TEXT;
ALTER TABLE device_events ADD COLUMN new_value TEXT;
ALTER TABLE device_events ADD COLUMN user_group TEXT;
