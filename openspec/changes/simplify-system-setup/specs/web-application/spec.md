# Spec Delta

## MODIFIED Requirements

### Requirement: Guided onboarding

The web application SHALL guide a user through creating a system, connecting a data source and verifying the first reading in one resumable flow in which only the system name is required. It SHALL open this flow automatically for a signed-in user who may manage systems and has none, and SHALL offer the same flow for every additional system.

#### Scenario: First system receives data

- **WHEN** a user completes the system step, creates a data source and the source or a test reading delivers data
- **THEN** the UI confirms ingestion, displays the normalized reading and the data source it arrived through, and links to uploader and troubleshooting documentation

#### Scenario: User without a system signs in

- **WHEN** a user who may manage systems and owns no system signs in
- **THEN** the application opens the guided setup instead of an empty dashboard

#### Scenario: Only the name is entered

- **WHEN** a user enters a system name and continues without any other input
- **THEN** the system is created and the flow proceeds to the data source step

#### Scenario: Uploader configuration is shown

- **WHEN** a user creates a data source in the setup
- **THEN** the UI shows the server URL, the system identifier, the credential and a profile-specific configuration, each with a copy action, and warns that the credential is shown only once

#### Scenario: Setup waits for data

- **WHEN** the verification step is open and no reading has arrived
- **THEN** the UI shows a waiting state that updates without a reload and offers to send a test reading or to finish without data

#### Scenario: Setup is resumed

- **WHEN** a user returns to a system whose setup was interrupted
- **THEN** the system's checklist offers to continue at the next open step

## ADDED Requirements

### Requirement: Per-system settings area

The web application SHALL provide for each system one settings area with the sections general, data sources, equipment, measurement channels, tariffs and sharing, each addressable by its own URL. Every system-specific setting SHALL be editable in exactly one of these sections and nowhere else.

#### Scenario: User edits a system

- **WHEN** a user with system management permission opens a system's settings
- **THEN** all six sections are reachable and the general section edits name, description, location, timezone, status interval, commissioning date and declared capacity

#### Scenario: User without management permission opens settings

- **WHEN** a user who may only read a system opens its settings
- **THEN** the sections are shown read-only without edit, create or delete actions

#### Scenario: Equipment is edited from administration

- **WHEN** a user opens the instance administration
- **THEN** it contains no system equipment editor and no per-system resource view

### Requirement: Data source management interface

The data sources section SHALL list each data source with its profile, connection state and last reading time, and SHALL let an authorized user add, rename, regenerate and delete data sources with confirmation before any action that invalidates a credential.

#### Scenario: Stale source is shown

- **WHEN** a data source has not delivered a reading within three status intervals
- **THEN** the list marks it as stale, shows the time of its last reading, and links to troubleshooting guidance

#### Scenario: Credential is regenerated

- **WHEN** a user confirms regeneration after being told the current credential stops working
- **THEN** the UI shows the new configuration once with copy actions

### Requirement: Measurement channel interface

The measurement channels section SHALL list every channel of the system with its key, unit, last value, last value time and binding, SHALL highlight unassigned channels, and SHALL let an authorized user bind a channel to the system, an inverter or a string, rename it, or retire it.

#### Scenario: Unassigned channel is bound

- **WHEN** a user binds an unassigned channel to a string and saves
- **THEN** the list shows the channel under that string and the setup status no longer counts it as unassigned

#### Scenario: Discovered equipment is offered

- **WHEN** a discovery proposal exists for the system
- **THEN** the section offers to review, adopt or dismiss the proposed inverter and strings

### Requirement: Tariff and sharing interfaces

The tariffs section SHALL let an authorized user create, edit and delete effective-dated import and export tariffs. The sharing section SHALL let an authorized user change the visibility, enable or disable the public link, copy it and rotate it with confirmation.

#### Scenario: Export tariff is added

- **WHEN** a user enters a currency, a price per kilowatt hour and an effective date for an export tariff and saves
- **THEN** the tariff is listed and the setup status reports the tariff item as done

#### Scenario: Public link is rotated

- **WHEN** a user confirms rotation of a public link
- **THEN** the UI shows the new link and states that the previous one no longer works

### Requirement: Setup checklist and honest data states

The web application SHALL show each system's setup status as a checklist with a link to the place where each open item is completed. It SHALL present a system without any reading as having no data yet and a system whose latest reading is older than three status intervals as stale, and SHALL NOT present either as live or online.

#### Scenario: Dashboard for a system without data

- **WHEN** a user opens the dashboard for a system that has never received a reading
- **THEN** the dashboard shows a no-data state with the recommended next step instead of live indicators and zero values

#### Scenario: Account without systems

- **WHEN** a user without any system opens the dashboard or a report
- **THEN** the view explains that no system exists and offers to add one if the user is permitted to

#### Scenario: Status in lists

- **WHEN** the system list or an equipment card shows a status
- **THEN** the status reflects whether readings are current, stale or absent

#### Scenario: Checklist is complete

- **WHEN** every setup item of a system is done or dismissed by the user
- **THEN** the checklist is no longer shown on the system page

### Requirement: Permission-aware navigation

The web application SHALL render a navigation entry only when the session holds the permission its destination requires, and SHALL name each entry after the view it opens.

#### Scenario: User cannot manage systems

- **WHEN** a user without system management permission views the navigation
- **THEN** no entry leads to a view that would respond with a forbidden page

#### Scenario: Former routes are opened

- **WHEN** a user opens a bookmarked former setup or administration equipment URL
- **THEN** the application redirects to the replacing view

### Requirement: Consistent setup terminology

The German and English user interface SHALL use exactly one term each for a PV system, a data source, an API key, a measurement channel and a notification target, in every label, message and help text.

#### Scenario: Locale files are checked

- **WHEN** the locale contract check runs
- **THEN** it fails if a retired term for one of these concepts appears in any user-facing string

### Requirement: Account API keys for general use

The account API key interface SHALL remain available for keys the user creates manually, SHALL list credentials that belong to data sources as managed entries that name their system and data source, and SHALL allow revoking but not editing managed entries.

#### Scenario: Managed credential is listed

- **WHEN** an account has a data source with an active credential
- **THEN** the API key list shows it with its system and data source name and a link to that data source

#### Scenario: User wants to connect an uploader

- **WHEN** a user opens the API key creation form
- **THEN** the form points to data sources as the way to connect an uploader
