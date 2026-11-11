# Timezone table

`zone.tab` is the country-to-timezone table of the IANA Time Zone Database,
release 2026f, copied unchanged. The file is in the public domain, as its
header states.

PVLog uses it to suggest the timezone of a new system from the country its
location lies in (`src/crates/application/src/timezone_by_country.rs`). Replace
the file with the one from a newer release to pick up new or renamed zones; the
zone names must be known to the bundled `chrono-tz` version.
