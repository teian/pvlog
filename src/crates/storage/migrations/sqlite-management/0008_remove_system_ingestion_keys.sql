-- Retires the superseded system-bound credential model after migration history is verified.
DROP TABLE system_ingestion_keys;
DROP INDEX system_registry_account_system_key;
