## ADDED Requirements

### Requirement: Team lifecycle and membership
The system SHALL let authorized users create and manage teams, join and leave teams with eligible systems, transfer team ownership, and enforce administrator-configurable membership rules and limits.

#### Scenario: System joins a team
- **WHEN** an authorized system manager joins an existing visible team and all eligibility rules pass
- **THEN** the membership becomes active and subsequent team aggregates include the system from the defined effective time

#### Scenario: Team owner attempts to leave
- **WHEN** the sole team owner attempts to leave without transferring or deleting the team
- **THEN** the system rejects the action with guidance and preserves team ownership

### Requirement: Privacy-aware discovery
The system SHALL support system and team search by permitted name, identifier, location granularity, country, capacity, and activity while excluding private systems and administrator-disabled discovery fields.

#### Scenario: Anonymous discovery is disabled
- **WHEN** the instance administrator disables public discovery
- **THEN** unauthenticated search returns no system or team records regardless of individual visibility flags

### Requirement: Favourites
Users SHALL be able to add and remove visible systems as favourites and list their favourites without gaining additional access rights to those systems.

#### Scenario: Favourite becomes private
- **WHEN** a favourited system is changed to private and the user has no membership
- **THEN** the system no longer exposes the system through the user's favourites and does not leak its current data

### Requirement: Team aggregates and ladders
The system SHALL produce period-based team totals, normalized comparisons, ranks, and system ladders from reconciled rollups with coverage and tie semantics.

#### Scenario: Incomplete member data affects rank
- **WHEN** a team member lacks sufficient coverage for a selected period
- **THEN** the ladder applies the documented eligibility policy and exposes the coverage reason instead of silently ranking incomplete data as complete

### Requirement: Regional supply views
The system SHALL expose configured regional electricity supply and demand series with stable region keys, timezone, resolution, delay, source, license/provenance, and freshness metadata.

#### Scenario: Regional provider is stale
- **WHEN** the latest supply data exceeds its configured freshness window
- **THEN** the API and UI label it stale with the last successful timestamp and do not present it as current

### Requirement: Privacy-safe cross-account projections
For SQLite deployments, cross-account discovery, team, favourite, comparison, and ladder queries SHALL use management-database projections containing only explicitly permitted fields and aggregate values, with per-account sequence and freshness metadata. Raw telemetry SHALL remain in the owning account database.

#### Scenario: Account makes a system private
- **WHEN** an account changes a projected system from public to private
- **THEN** the management projection is invalidated before the system can remain discoverable and subsequent cross-account reads expose no private fields

#### Scenario: Projection is behind
- **WHEN** a cross-account result is based on a projection older than its source account checkpoint
- **THEN** the response identifies the projection freshness or excludes it according to the configured consistency policy
