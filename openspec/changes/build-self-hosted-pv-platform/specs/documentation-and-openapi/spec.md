## ADDED Requirements

### Requirement: Complete OpenAPI 3.1 contract
The repository SHALL contain `openapi/pvlog-v1.yaml` as a valid OpenAPI 3.1 document describing every modern REST operation, parameter, request body, response, schema, security requirement, scope, error, callback/webhook, example, operation identifier, tag, and deprecation state.

#### Scenario: OpenAPI validation runs in CI
- **WHEN** a change modifies a modern route, DTO, security rule, or response
- **THEN** CI validates the OpenAPI document and fails if the implemented and committed operation/schema surfaces drift

### Requirement: Rendered API reference
The system SHALL serve a version-matched interactive API reference generated from the committed OpenAPI document and SHALL offer the raw YAML without requiring external CDN assets.

#### Scenario: Offline self-hosted docs are opened
- **WHEN** an operator opens the documentation on an installation without internet access
- **THEN** the API reference, schemas, and examples render from locally packaged assets and link to the exact served OpenAPI file

### Requirement: Task-oriented guides
The documentation SHALL include tested quickstarts for installation, authentication, creating a system, uploading single/batch status, correcting data, querying charts/statistics, pagination, errors/rate limits, webhooks, import/export, backup/restore, upgrades, and troubleshooting.

#### Scenario: Quickstart is tested
- **WHEN** documentation conformance tests execute against an ephemeral release instance
- **THEN** every marked command/request example succeeds with its documented status and response shape

### Requirement: PVOutput compatibility reference
The documentation SHALL publish a dated compatibility matrix for every official r2 service, parameter, response, constraint, error, donation feature, and intentional difference, plus a migration guide for existing PVOutput uploaders.

#### Scenario: Operator migrates an uploader
- **WHEN** an operator follows the compatibility guide for a documented uploader
- **THEN** the guide identifies the base URL, credential mapping, supported endpoint, security caveats, and a verifiable test request

### Requirement: Documentation quality and navigation
Reference content SHALL define every field and unit once, cross-link concepts, provide copyable examples in curl and at least one generated client, include a glossary and searchable navigation, identify required permissions, and distinguish normative guarantees from operational recommendations.

#### Scenario: Reader opens an operation
- **WHEN** a reader views any modern API operation
- **THEN** they can find its purpose, authorization, parameters, units, limits, success/error examples, idempotency/concurrency behavior, and related guide without consulting source code

### Requirement: Release and change documentation
Every release that changes a public API or storage/export format SHALL publish a changelog entry, compatibility classification, migration action, and support/deprecation timeline.

#### Scenario: Breaking API change is proposed
- **WHEN** a public field or behavior would become incompatible
- **THEN** documentation checks require a new major API contract and migration guidance before release

### Requirement: Documentation accessibility and freshness
The documentation site SHALL be keyboard accessible, responsive, readable in light/dark modes, and versioned with the application. Automated link, spelling/terminology, OpenAPI, snippet, and accessibility checks SHALL run in CI.

#### Scenario: Stale internal link is introduced
- **WHEN** a documentation change references a missing local page or anchor
- **THEN** CI fails with the broken source and target

