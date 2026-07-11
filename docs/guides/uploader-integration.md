# Uploader integration and functional coverage

Use the modern base URL `$PVLOG_URL/api/v1`; no PVOutput-compatible wire layer is
provided. Obtain a scoped system credential, store it outside logs/config files,
send HTTPS requests, use explicit timestamps/units, and assign a stable
idempotency key per source observation. Back off according to `Retry-After` and
never retry schema or authorization failures unchanged.

| Capability                                | API              | Web UI        | Notes                    |
| ----------------------------------------- | ---------------- | ------------- | ------------------------ |
| System aggregate over inverter strings    | Yes              | Yes           | Effective-dated capacity |
| Single/batch telemetry and correction     | Yes              | Yes           | Canonical units only     |
| Raw, rollup, statistics, quality          | Yes              | Yes           | Gaps are not fabricated  |
| Local/OIDC/OAuth2 authentication and RBAC | Yes              | Yes           | Provider-neutral backend |
| Alerts and signed webhooks                | Yes              | Admin summary | SSRF-safe delivery       |
| Import/export and backup                  | Operator command | Admin summary | Checksummed manifests    |

Minimal telemetry request:

```sh
curl -fsS -X POST "$PVLOG_URL/api/v1/systems/$SYSTEM_ID/observations" \
  -H "Authorization: Bearer $PVLOG_TOKEN" \
  -H "Idempotency-Key: $SOURCE_EVENT_ID" \
  -H 'Content-Type: application/json' \
  --data-binary @observation.json
```
