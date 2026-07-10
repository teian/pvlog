## ADDED Requirements

### Requirement: Complete documented r2 service surface
The system SHALL expose compatible routes under `/service/r2` for `addoutput.jsp`, `addstatus.jsp`, `addbatchstatus.jsp`, `getstatus.jsp`, `getstatistic.jsp`, `getsystem.jsp`, `postsystem.jsp`, `getladder.jsp`, `getoutput.jsp`, `getextended.jsp`, `getfavourite.jsp`, `getmissing.jsp`, `getinsolation.jsp`, `deletestatus.jsp`, `search.jsp`, `getteam.jsp`, `jointeam.jsp`, `leaveteam.jsp`, `getsupply.jsp`, `registernotification.jsp`, and `deregisternotification.jsp`.

#### Scenario: Compatibility inventory is tested
- **WHEN** the compatibility conformance suite enumerates the official service inventory
- **THEN** every documented r2 service resolves to an implemented handler and has golden success, authentication, validation, and authorization cases

### Requirement: Legacy request compatibility
Each compatibility service SHALL accept its documented HTTP methods, `X-Pvoutput-Apikey` and `X-Pvoutput-SystemId` headers, supported `key` and `sid` query alternatives, form/query parameter names, date/time formats, booleans, numbered extended values, CSV data parameters, batching formats, and optional flags.

#### Scenario: Existing uploader posts a status
- **WHEN** an uploader sends a documented `addstatus.jsp` form request using legacy field names and header authentication
- **THEN** the system maps the request to canonical telemetry semantics and returns the documented legacy success format

#### Scenario: Legacy GET mutation is disabled by policy
- **WHEN** an administrator has disabled credential-bearing GET mutations and a client attempts one
- **THEN** the service returns a documented compatibility difference and points operators to the safer POST configuration

### Requirement: Output, status, and extended data behavior
The compatibility API SHALL implement documented daily output upload, live status upload, batch status upload, cumulative energy, power/energy calculation, net data, battery state, extended values, output/status history, day statistics, aggregate output, team output, missing data, and deletion semantics.

#### Scenario: Batch status contains per-item results
- **WHEN** a documented legacy batch contains accepted and rejected status records
- **THEN** the response uses the documented per-record status ordering and no record is counted twice

### Requirement: System, search, favourite, and community behavior
The compatibility API SHALL implement documented system reads/updates, system search and country filters, favourites, teams, join/leave rules, ladders, visibility enforcement, and inaccessible-system errors.

#### Scenario: Private system is requested by another principal
- **WHEN** a compatibility request targets a private system without an authorized relationship
- **THEN** the service returns the documented inaccessible-system behavior without leaking private metadata

### Requirement: Insolation, supply, and notification behavior
The compatibility API SHALL implement documented insolation parameters and timezone handling, region supply keys and history flags, notification registration/deregistration, alert types, and callback payload mapping. Unavailable optional provider data SHALL produce an explicit documented service error rather than fabricated values.

#### Scenario: Notification is registered
- **WHEN** a valid legacy application identifier, callback URL, and alert type are registered
- **THEN** the system stores the registration under the authenticated system and sends matching callbacks using the documented legacy payload fields

### Requirement: Legacy response and error compatibility
Compatibility handlers SHALL reproduce documented success text/CSV field order, delimiters, empty values, status codes, common authentication errors, read-only errors, validation errors, and optional rate-limit response headers. Modern problem JSON SHALL NOT replace a documented legacy body on these routes.

#### Scenario: Legacy API key is missing
- **WHEN** a compatibility service requires authentication and no valid API key is supplied
- **THEN** the service returns the documented legacy unauthorized status and text format

### Requirement: Documented features without donation gates
The self-hosted system SHALL make all data features described as donation features available subject only to administrator-configured safety and capacity policy; it SHALL NOT emulate an external donation or subscription entitlement.

#### Scenario: Administrator raises a legacy batch limit
- **WHEN** a documented donation-mode batch feature is enabled within administrator limits
- **THEN** the compatibility service accepts it without requiring any hosted-service payment state

### Requirement: Dated compatibility matrix
The project SHALL maintain a machine-readable and human-readable matrix pinned to the official PVOutput API documentation snapshot, mapping every service, parameter, constraint, response field, error, and intentional difference to implementation and conformance tests.

#### Scenario: Official documentation changes
- **WHEN** a review detects a change in the official API documentation after the pinned snapshot
- **THEN** the matrix records the delta and the project explicitly accepts, defers, or rejects it before claiming compatibility with the newer snapshot

