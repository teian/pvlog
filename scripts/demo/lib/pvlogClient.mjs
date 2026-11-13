/** A small client for the PVLog API: a browser session, or a data source key. */

import { gzipSync } from "node:zlib";

/** Fails with the problem document of a refused request. */
async function failure(response, what) {
  const text = await response.text();
  return new Error(
    `${what} failed with ${response.status}: ${text.slice(0, 500)}`,
  );
}

/**
 * Signs in with a local password and keeps the session cookie and CSRF token.
 *
 * @param {string} baseUrl
 * @param {string} email
 * @param {string} password
 */
export async function signIn(baseUrl, email, password) {
  const response = await fetch(`${baseUrl}/api/v1/auth/local/login`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ email, password }),
  });
  if (!response.ok) throw await failure(response, "sign-in");
  const cookie = response.headers
    .getSetCookie()
    .map((value) => value.split(";")[0])
    .join("; ");
  let csrf = response.headers.get("x-csrf-token");
  const session = {
    /** Sends a JSON request in the session. */
    async request(method, path, body) {
      const headers = { cookie, accept: "application/json" };
      if (body !== undefined) headers["content-type"] = "application/json";
      if (method !== "GET" && csrf) headers["x-csrf-token"] = csrf;
      const reply = await fetch(`${baseUrl}${path}`, {
        method,
        headers,
        body: body === undefined ? undefined : JSON.stringify(body),
      });
      if (!reply.ok) throw await failure(reply, `${method} ${path}`);
      csrf = reply.headers.get("x-csrf-token") ?? csrf;
      return reply.status === 204 ? null : reply.json();
    },
  };
  await session.request("GET", "/api/v1/session");
  return session;
}

/**
 * Delivers an ingestion request with a data source key, compressed, retrying
 * while the server asks to wait.
 *
 * @param {{ baseUrl: string, systemId: string, apiKey: string }} target
 * @param {object} body
 */
export async function ingest(target, body) {
  const payload = gzipSync(JSON.stringify({ schemaVersion: 1, ...body }));
  for (let attempt = 1; ; attempt += 1) {
    const response = await fetch(
      `${target.baseUrl}/api/v1/systems/${target.systemId}/ingest`,
      {
        method: "POST",
        headers: {
          authorization: `Bearer ${target.apiKey}`,
          "content-type": "application/json",
          "content-encoding": "gzip",
        },
        body: payload,
      },
    );
    if (response.ok) return response.json();
    if ((response.status === 429 || response.status === 503) && attempt < 8) {
      const wait = Number(response.headers.get("retry-after") ?? 2);
      await Bun.sleep(Math.max(1, wait) * 1000);
      continue;
    }
    throw await failure(response, "ingest");
  }
}
