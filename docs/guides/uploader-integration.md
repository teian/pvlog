# Uploader integration

PVLog uses the same account API keys for every non-interactive integration.
Open **Account API keys**, create a named key with only `telemetry:write`, and
copy the cleartext value when it is shown. It cannot be displayed again.

Store the key in a secret manager or a mode-`0600` environment file, never in
source control, shell history, screenshots, logs, or automation exports. Use
HTTPS outside isolated local development. Authentication uses only the standard
`Authorization: Bearer` header; custom upload-key headers, credential-bearing
push URLs, and query credentials are not supported.

## Shell uploader

Load `PVLOG_URL`, `PVLOG_SYSTEM_ID`, and `PVLOG_API_KEY` from a protected secret
file, then send a reading with an idempotency key that stays stable for the
source reading:

```sh
set -a
. /run/secrets/pvlog-uploader.env
set +a

curl --fail-with-body \
  -H "Authorization: Bearer $PVLOG_API_KEY" \
  -H "Idempotency-Key: meter-1780000000000" \
  -H 'Content-Type: application/json' \
  --data '{"observedAtEpochMillis":1780000000000,"generationPowerWatts":4200}' \
  "$PVLOG_URL/api/v1/systems/$PVLOG_SYSTEM_ID/observations"
```

For a batch, post the canonical batch document to
`/api/v1/systems/$PVLOG_SYSTEM_ID/observations/batch` with the same bearer
header. Each item carries its own `idempotencyKey`.

## Home Assistant example

Keep the key in `secrets.yaml` as `pvlog_api_key`, then reference it from a REST
command. Avoid logging resolved headers or request payloads.

```yaml
rest_command:
  pvlog_observation:
    url: "https://pvlog.example/api/v1/systems/SYSTEM_ID/observations"
    method: post
    headers:
      authorization: "Bearer {{ pvlog_api_key }}"
      content-type: "application/json"
      idempotency-key: "{{ source_event_id }}"
    payload: >-
      {"observedAtEpochMillis": {{ observed_at_ms }},
       "generationPowerWatts": {{ generation_watts }}}
```

## Revocation and recovery

Create a separate API key for each uploader so one integration can be revoked
without interrupting another. Revocation never deletes observations already
accepted. Back off according to `Retry-After`; do not retry schema or
authorization failures unchanged.

If a key leaks:

1. Revoke it immediately under **Account API keys**.
2. Create a replacement with only `telemetry:write`.
3. Replace the secret at the uploader and verify a successful upload.
4. Search audit and access logs by safe credential ID and time window, never by
   pasting the leaked secret into a search tool.
