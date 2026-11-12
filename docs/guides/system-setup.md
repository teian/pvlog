# Setting up a system

This guide follows the web application from the first sign-in to a system that
receives data. Labels in bold are the ones the English interface shows.

## Overview

A system needs only a name to exist. Everything else is optional and can be
added later in the system's settings:

| What                  | Needed for                                             | Where                                             |
| --------------------- | ------------------------------------------------------ | ------------------------------------------------- |
| Name                  | Creating the system                                    | Setup, step 1                                     |
| Location              | Timezone, weather data, yield forecast                 | Setup, step 1, or **Settings** → **General**      |
| Installed capacity    | Yield per kWp, until strings are entered               | Setup, step 1, or **Settings** → **General**      |
| Data source           | Receiving readings from a logger                       | Setup, step 2, or **Settings** → **Data sources** |
| Inverters and strings | Per-string charts and a more precise forecast          | **Settings** → **Equipment**                      |
| Measurement channels  | Naming and assigning the values a data source delivers | **Settings** → **Measurement channels**           |
| Tariffs               | Revenue and costs                                      | **Settings** → **Tariffs**                        |
| Sharing               | Visibility and the public link                         | **Settings** → **Sharing**                        |

## The guided setup

Someone who may manage systems and has none yet lands in the setup directly
after signing in. Later systems are added with **Add system**, which the
systems list and the sidebar both offer. The setup has three steps:
**System**, **Data source** and **First data**.

### Step 1: System

1. Enter the **System name**. It is the only required field.
2. Optionally search for the **Location** and pick a suggestion. PVLog takes
   the timezone from the country of the location; for a country with several
   timezones it uses the one closest to the chosen place. Without a location
   the timezone of the browser is used.
3. Optionally enter the **Installed capacity (kWp)**.
4. **More options** holds the **Timezone**, the **Seconds between readings**,
   the **Commissioning date** and the **Description**. Set the timezone here if
   the derived one is wrong, which can happen close to a timezone boundary
   inside a country.
5. Choose **Create system and continue**.

The system exists from this point on. Leaving the setup loses nothing; the
remaining steps can be done from the system's settings at any time.

### Step 2: Data source

1. Pick the logger under **Which logger sends the data?**: **smalog**,
   **SBFspot**, **Other logger** or **Import**.
2. Choose **Create data source**.
3. PVLog shows the **Server URL**, the **System ID**, the **API key** and a
   ready-made configuration for the chosen logger. Copy them now: the key is
   shown only this once, and PVLog stores only its hash.
4. Put the configuration into the logger, then choose **I have copied the
   configuration**.

The key of a data source can upload readings for this one system and nothing
else. **Connect later** skips the step; the system then shows a data source as
its next step until one exists. The **Import** profile needs no key.

How each logger uses the configuration is described in
[Uploader integration](uploader-integration.md) and
[SBFspot push service](sbfspot-push-service.md).

### Step 3: First data

The page waits for the first reading and updates by itself. Once a reading
arrives it shows when it was measured, through which data source it came and
its values, and **Open system** leads to the system.

**Send test reading** checks the connection before the logger runs. A test
reading is stored apart from the telemetry: it never appears in a total,
statistic or forecast, and the setup keeps waiting for a real reading.
**Finish without data** leaves the setup; the system then reports that it has
no data yet.

## What a system shows until it is set up

PVLog never presents a system as live when it is not:

- **No data yet**: no reading has ever arrived. The dashboard and the reports
  show this state with the next step instead of zero values.
- **Receiving data**: the newest reading is at most three reporting intervals
  old.
- **No recent data**: the newest reading is older than that. Values are shown
  with their age and are not marked as live.

The systems list shows this state for every system, and the sidebar marks
systems that are not delivering.

While something is still open, the system page shows **Finish setting up**: a
list of the open steps, each with a link to the settings section where it is
done. Only connecting a data source and receiving the first reading are marked
as **Next step**; everything else is optional. A step that is not needed can be
hidden, and the list disappears once every step is done or hidden. Hidden steps
are remembered per browser.

## Changing a system later

Open the system and choose **Settings**. Each section has its own address, so
it can be bookmarked:

- **General**: name, location, timezone, reporting interval, commissioning
  date and installed capacity. **Archive system** and **Delete system** are at
  the end of this section.
- **Data sources**: **Add data source**, rename, **Regenerate key** and
  **Delete**. Regenerating invalidates the previous key at once; the logger
  cannot upload until it has the new one. Deleting a data source keeps the
  readings it delivered.
- **Equipment**: inverters and their strings. Once strings exist, their sum
  replaces the installed capacity entered under **General**.
- **Measurement channels**: the individual values a data source delivers. A
  channel that delivers values but is **Not assigned** can be bound to an
  inverter or string here.
- **Tariffs**: **Add tariff** for exported or imported energy.
- **Sharing**: visibility and the public link. **Create a new link** replaces
  the link; the previous one stops working at once.

Someone who may view a system but not manage it sees the same sections without
the actions.

## API keys

Loggers use the key of their data source. For other integrations, open
**Account** in the sidebar and create a key under **Account API keys** with
only the permissions the integration needs. Keys that belong to a data source
are listed there as well, but are managed in the system's settings.

## Former addresses

Bookmarks of the earlier setup and management pages keep working: `/onboarding`
leads to the setup and `/systems/manage` to the systems list. Equipment is no
longer edited under Administration.
