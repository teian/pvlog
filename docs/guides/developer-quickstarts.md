# Developer quickstarts

## Authentication and authorization

Create the first local administrator through the instance bootstrap flow, sign
in with the browser session endpoint, and assign the narrowest account or system
role required. API clients use a one-time-displayed bearer credential with
explicit scopes. Recovery tokens are single use; identity linking requires a
recent authenticated session. Never infer an account from untrusted input.

For OIDC, register the exact PVLog callback URL, configure discovery issuer,
client ID, secret reference, scopes, and claim mappings, then run connector
validation. Generic OAuth2 uses explicit authorization, token, and user-info
endpoints. Google, GitHub, Facebook, and X are versioned setup presets, not
provider-specific backend types. Always verify `state`; OIDC additionally uses
nonce and PKCE. Preserve a local recovery path before linking the last identity.

## Create a system and ingest telemetry

```sh
curl -fsS -X POST "$PVLOG_URL/api/v1/systems" \
  -H "Authorization: Bearer $PVLOG_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{"name":"Roof","timezone":"Europe/Berlin"}'
```

Create inverters with nested strings under the returned system. Send canonical
observations with explicit units and an `Idempotency-Key`. A repeated key with
the same body replays the result; a changed body conflicts. Corrections use the
observation resource and an ETag precondition.

## Query, charts, pagination, and errors

Request only the fields and resolution needed, provide the display timezone,
and respect the maximum point budget. Missing intervals remain gaps. Cursor
pagination binds the cursor to filters and sort order. Errors use
`application/problem+json`; honor `Retry-After` on throttling or backpressure.
CSV and JSON exports share the same query semantics as charts.

## Generated clients

Generate clients from `openapi/pvlog-v1.yaml`, pin the contract version in the
consumer build, and regenerate on version changes. Treat unknown enum values and
new optional fields defensively. A generated client must still protect bearer
tokens, supply idempotency keys for retries, and validate webhook signatures.
