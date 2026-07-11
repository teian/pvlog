# Local authentication and RBAC quickstart

This guide uses the browser-session endpoints exposed by a local PVLog server.
The examples assume the server listens on `http://127.0.0.1:8080`.

## Sign in and retain the browser session

Authenticate with a local user. Store the response cookie in a protected
temporary cookie jar; do not copy it into scripts, logs, or source control.

```sh
curl --fail-with-body -D response-headers.txt -c pvlog.cookies \
  -H 'content-type: application/json' \
  --data '{"email":"admin@example.test","password":"replace-with-password"}' \
  http://127.0.0.1:8080/api/v1/auth/local/login
```

The response sets the `__Host-pvlog_session` HttpOnly cookie and includes an
`X-CSRF-Token` response header. Read the header value into a process-local
variable before issuing a state-changing request:

```sh
csrf_token="$(awk 'BEGIN{IGNORECASE=1} /^x-csrf-token:/{print $2}' response-headers.txt | tr -d '\r')"
test -n "$csrf_token"
```

`GET /api/v1/session` returns the active user, selected account, visible
system IDs, and configured login choices. It is safe to call without a cookie;
the anonymous response has `authenticated: false`.

```sh
curl --fail-with-body -b pvlog.cookies \
  http://127.0.0.1:8080/api/v1/session
```

## Create a constrained account role

Only a session user who has account-scoped `role_manage` can read or mutate the
role catalog. API credentials may read the catalog only when their scope and
RBAC assignment allow it; role mutations require a browser session.

```sh
account_id='replace-with-account-uuid-v7'
curl --fail-with-body -b pvlog.cookies \
  -H "x-csrf-token: $csrf_token" \
  -H 'content-type: application/json' \
  --data '{"name":"Telemetry analyst","permissions":["telemetry_read","audit_read"]}' \
  "http://127.0.0.1:8080/api/v1/accounts/$account_id/roles"
```

The server rejects permissions the actor cannot delegate. Treat role IDs as
opaque UUIDv7 values and use the returned ID to assign the role.

## Assign a role at account or system scope

To assign an existing role to a user, send its UUIDv7 and the target user ID.
Omit `systemId` for account scope; include it only for a system-scoped grant.

```sh
role_id='replace-with-role-uuid-v7'
user_id='replace-with-user-uuid-v7'
curl --fail-with-body -b pvlog.cookies \
  -H "x-csrf-token: $csrf_token" \
  -H 'content-type: application/json' \
  --data "{\"roleId\":\"$role_id\",\"principalType\":\"user\",\"principalId\":\"$user_id\"}" \
  "http://127.0.0.1:8080/api/v1/accounts/$account_id/role-assignments"
```

The `principalType` is either `user` or `api_credential`. All scopes and
delegation constraints are re-evaluated server-side; client-selected IDs never
bypass RBAC.

## Invite and activate a local user

An instance administrator can create an invitation. The activation token is
returned exactly once. Deliver it using an approved secure channel and do not
put it in browser history, tickets, logs, analytics, or source control.

```sh
curl --fail-with-body -b pvlog.cookies \
  -H "x-csrf-token: $csrf_token" \
  -H 'content-type: application/json' \
  --data '{"email":"new.user@example.test"}' \
  http://127.0.0.1:8080/api/v1/admin/user-invitations
```

The invitee accepts the token and selects a password that meets the deployment's
configured local password policy. Acceptance atomically activates the user and
stores its Argon2id verifier.

```sh
curl --fail-with-body -H 'content-type: application/json' \
  --data '{"token":"one-time-token","displayName":"New user","password":"replace-with-policy-compliant-password"}' \
  http://127.0.0.1:8080/api/v1/auth/invitations/accept
```

The endpoint deliberately returns the same accepted response for unknown,
expired, and consumed tokens after syntactically valid input, reducing account
and invitation enumeration.

## Inspect external identity links and connector metadata

The signed-in browser user can inspect only their own identity links:

```sh
curl --fail-with-body -b pvlog.cookies \
  http://127.0.0.1:8080/api/v1/users/me/identities
```

Instance administrators can inspect configured connector metadata, but the
response deliberately omits client IDs, client-secret references, token
endpoints, and claim mappings:

```sh
curl --fail-with-body -b pvlog.cookies \
  http://127.0.0.1:8080/api/v1/admin/auth-connectors
```

## Sign out

Send the CSRF header when revoking a browser session. Delete the local cookie
jar and captured CSRF header after a successful `204 No Content` response.

```sh
curl --fail-with-body -X POST -b pvlog.cookies \
  -H "x-csrf-token: $csrf_token" \
  http://127.0.0.1:8080/api/v1/session
rm -f pvlog.cookies response-headers.txt
unset csrf_token
```
