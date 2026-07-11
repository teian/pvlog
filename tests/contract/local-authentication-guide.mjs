import { readFileSync } from "node:fs";

const guide = readFileSync("docs/guides/local-authentication-rbac.md", "utf8");
const specification = readFileSync("openapi/pvlog-v1.yaml", "utf8");
const guideRoutes = [
  "/api/v1/auth/local/login",
  "/api/v1/session",
  "/api/v1/accounts/{account_id}/roles",
  "/api/v1/accounts/{account_id}/role-assignments",
  "/api/v1/admin/user-invitations",
  "/api/v1/auth/invitations/accept",
  "/api/v1/users/me/identities",
  "/api/v1/admin/auth-connectors",
];

for (const route of guideRoutes) {
  const guideRoute = route.replace("{account_id}", "$account_id");
  if (!guide.includes(guideRoute))
    throw new Error(
      `local authentication guide does not mention ${guideRoute}`,
    );
  if (!specification.includes(`  ${route}:`))
    throw new Error(`OpenAPI contract does not document ${route}`);
}

console.log(
  `Local authentication guide covers ${guideRoutes.length} documented routes`,
);
