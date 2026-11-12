# Uploader integration

The steps in the web application, from creating a system to its first reading,
are described in [Setting up a system](system-setup.md).

## Connect an uploader with a data source

The usual way to connect a logger is a **data source**. Open the system, go to
**Settings → Data sources**, choose **Add data source**, name it and pick the
profile of your logger. PVLog then shows, once, everything the logger needs:

- the server URL and the system identifier,
- an API key that can only upload readings for this one system,
- a configuration for the chosen profile with every value filled in.

Copy the configuration before closing the dialog. The key cannot be displayed
again; **Regenerate** issues a new one and invalidates the old one at once.

| Profile   | Configuration shown                                                                                                     |
| --------- | ----------------------------------------------------------------------------------------------------------------------- |
| `smalog`  | A `[pvlog]` section for `config.toml`, which refers to the key as `${PVLOG_API_KEY}`, and the matching environment line |
| `sbfspot` | An environment file for [`sbfspot-push`](sbfspot-push-service.md)                                                       |
| `generic` | A complete `curl` call and the raw HTTP request                                                                         |
| `import`  | None; this profile is for history imports and needs no key                                                              |

The same can be done over the API:

```sh
curl --fail-with-body -X POST \
  -H "Authorization: Bearer $PVLOG_MANAGEMENT_KEY" \
  -H 'Content-Type: application/json' \
  --data '{"name":"Roof logger","profile":"smalog"}' \
  "$PVLOG_URL/api/v1/systems/$PVLOG_SYSTEM_ID/data-sources"
```

The response carries `configuration.apiKey` and `configuration.snippets` and is
sent with `Cache-Control: no-store`. A key that is itself restricted to one
system cannot create or change data sources.

Each data source reports a state: `never_connected` until its first accepted
reading, `receiving`, `stale` once the last reading is older than three status
intervals of the system, and `disconnected` when its key was revoked.

To check the connection before the logger runs, send a test reading from the
data source's entry, or let the logger send one reading with `"test": true`.
Test readings are validated like real ones but stored apart from telemetry, so
they never appear in a total, statistic or forecast, and **Remove test
readings** deletes only them.

## Account API keys for other integrations

Integrations that are not tied to one data source use an account API key. Open
**Account** in the sidebar, go to **Account API keys**, create a named key with only the permissions the
integration needs, and copy the cleartext value when it is shown. It cannot be
displayed again. An account key with `telemetry:write` can upload for every
system of the account, which is why a data source key is the better choice for
a logger.

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

Every data source has its own key, so one logger can be cut off without
interrupting another. Revocation never deletes observations already
accepted. Back off according to `Retry-After`; do not retry schema or
authorization failures unchanged.

If a key leaks:

1. For a data source key, choose **Regenerate key** under **Settings → Data
   sources**; the old key stops working at once. Revoke an account key under
   **Account → Account API keys** and create a replacement with only
   `telemetry:write`.
2. Replace the secret at the uploader and verify a successful upload.
3. Search audit and access logs by safe credential ID and time window, never by
   pasting the leaked secret into a search tool.
