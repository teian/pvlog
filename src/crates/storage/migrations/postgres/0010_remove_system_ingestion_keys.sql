-- Retires the superseded system-bound credential model after migration history is verified.
DROP TABLE management.system_ingestion_keys;
DROP INDEX management.system_registry_account_system_key;
