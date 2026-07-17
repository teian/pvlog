# Open PV Module Database attribution

PVLog's bundled module catalog includes normalized factual records imported from
the Open PV Module Database snapshot generated on 2026-07-12 at commit
`dabecc6aaf0d835fbfb104f7cb2f4e1ea88dbf68`.

- Database project: Open PV Module Database
- Imported source: `dist/modules.json`
- Database license notice: CC BY 4.0 where contributor-owned; upstream source
  terms and factual-data rights remain applicable
- Principal upstream catalog: CEC/SAM module catalogue from NLR/NREL SAM
- Import command:
  `pnpm catalog:import-open-pv-modules /path/to/open-pv-module-database`

Every imported entry retains its source name, source URL, and verification date.
PVLog converts units deterministically and rejects records that fail its
electrical consistency checks. Missing published values remain absent rather
than being represented as zero or inferred defaults.
