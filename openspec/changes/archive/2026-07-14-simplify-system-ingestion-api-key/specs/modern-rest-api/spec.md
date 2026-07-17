## ADDED Requirements

### Requirement: Stable simple uploader endpoints

The API SHALL support system-key ingestion through the canonical system observation routes using `x-pvlog-api-key` and through generated routes shaped as `/api/v1/push/{system_id}/{ingestion_key}/observations` and `/api/v1/push/{system_id}/{ingestion_key}/observations/batch`. The push-route credential SHALL be opaque, SHALL NOT be a query parameter, and SHALL NOT alter request or response telemetry schemas.

#### Scenario: Header endpoint is documented

- **WHEN** an account user or uploader opens the API documentation
- **THEN** the documentation provides a copyable `curl` example using `x-pvlog-api-key`, JSON content type, idempotency key, and the canonical system observation URL

#### Scenario: Push URL endpoint is documented

- **WHEN** an account user creates an ingestion key
- **THEN** the system provides complete single and batch push URLs whose only required uploader inputs are the JSON body and normal content/idempotency headers

#### Scenario: Invalid URL key is submitted

- **WHEN** a push URL contains an unknown, malformed, expired, or revoked key
- **THEN** the API returns the same generic authentication problem without disclosing credential or system validity and without reflecting the credential in the response

#### Scenario: Existing bearer client continues ingestion

- **WHEN** an existing client uses a valid scoped bearer API token on the canonical ingestion endpoint
- **THEN** the API retains its documented behavior without requiring migration to a system ingestion key
