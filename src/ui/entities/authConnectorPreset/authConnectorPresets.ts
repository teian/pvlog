import type { AuthConnectorPreset } from "./authConnectorPreset.types";

/** Version of the complete administrator-facing preset catalog. */
export const AUTH_CONNECTOR_PRESET_CATALOG_VERSION = "2026.07.10";

/**
 * Maintained identity connector templates verified against official provider documentation.
 *
 * Provider branding intentionally stops at this UI catalog boundary. Protocol settings remain
 * generic and can be edited after applying a preset.
 */
export const AUTH_CONNECTOR_PRESETS = [
  {
    id: "google",
    revision: 1,
    verifiedAt: "2026-07-10",
    display: {
      name: "Google",
      descriptionKey: "features.authConnectorPresets.google.description",
      iconKey: "google",
      setupUrl: "https://console.cloud.google.com/apis/credentials",
      documentationUrl:
        "https://developers.google.com/identity/openid-connect/openid-connect",
    },
    configuration: {
      protocol: "oidc",
      issuer: "https://accounts.google.com",
      scopes: ["openid", "profile", "email"],
    },
  },
  {
    id: "github",
    revision: 1,
    verifiedAt: "2026-07-10",
    display: {
      name: "GitHub",
      descriptionKey: "features.authConnectorPresets.github.description",
      iconKey: "github",
      setupUrl: "https://github.com/settings/developers",
      documentationUrl:
        "https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps",
    },
    configuration: {
      protocol: "oauth2",
      authorizationEndpoint: "https://github.com/login/oauth/authorize",
      tokenEndpoint: "https://github.com/login/oauth/access_token",
      userInfoEndpoint: "https://api.github.com/user",
      clientAuthentication: "requestBody",
      scopes: ["read:user", "user:email"],
      claimMappings: {
        subject: "id",
        displayName: "name",
        email: "email",
        avatarUrl: "avatar_url",
      },
    },
  },
  {
    id: "facebook",
    revision: 1,
    verifiedAt: "2026-07-10",
    display: {
      name: "Facebook",
      descriptionKey: "features.authConnectorPresets.facebook.description",
      iconKey: "facebook",
      setupUrl: "https://developers.facebook.com/apps/",
      documentationUrl:
        "https://developers.facebook.com/docs/facebook-login/guides/advanced/manual-flow/",
    },
    configuration: {
      protocol: "oauth2",
      authorizationEndpoint: "https://www.facebook.com/v25.0/dialog/oauth",
      tokenEndpoint: "https://graph.facebook.com/v25.0/oauth/access_token",
      userInfoEndpoint:
        "https://graph.facebook.com/v25.0/me?fields=id,name,email,picture",
      clientAuthentication: "requestBody",
      scopes: ["public_profile", "email"],
      claimMappings: {
        subject: "id",
        displayName: "name",
        email: "email",
        avatarUrl: "picture.data.url",
      },
    },
  },
  {
    id: "x",
    revision: 1,
    verifiedAt: "2026-07-10",
    display: {
      name: "X",
      descriptionKey: "features.authConnectorPresets.x.description",
      iconKey: "x",
      setupUrl: "https://developer.x.com/en/portal/dashboard",
      documentationUrl:
        "https://docs.x.com/fundamentals/authentication/oauth-2-0/authorization-code",
    },
    configuration: {
      protocol: "oauth2",
      authorizationEndpoint: "https://x.com/i/oauth2/authorize",
      tokenEndpoint: "https://api.x.com/2/oauth2/token",
      userInfoEndpoint:
        "https://api.x.com/2/users/me?user.fields=confirmed_email,profile_image_url",
      clientAuthentication: "basicAuth",
      scopes: ["users.read", "users.email", "offline.access"],
      claimMappings: {
        subject: "data.id",
        displayName: "data.name",
        email: "data.confirmed_email",
        avatarUrl: "data.profile_image_url",
      },
    },
  },
] as const satisfies readonly AuthConnectorPreset[];
