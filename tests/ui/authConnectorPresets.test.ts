import { describe, expect, it } from "vitest";

import {
  AUTH_CONNECTOR_PRESETS,
  AUTH_CONNECTOR_PRESET_CATALOG_VERSION,
  createConnectorDraft,
} from "@/entities/authConnectorPreset";
import i18n from "@/shared/lib/i18n";

describe("authentication connector preset catalog", () => {
  it("keeps a dated, versioned, localized administrator catalog", () => {
    expect(AUTH_CONNECTOR_PRESET_CATALOG_VERSION).toBe("2026.07.10");
    expect(AUTH_CONNECTOR_PRESETS.map(({ id }) => id)).toEqual([
      "google",
      "github",
      "facebook",
      "x",
    ]);

    for (const preset of AUTH_CONNECTOR_PRESETS) {
      expect(preset.revision).toBeGreaterThan(0);
      expect(preset.verifiedAt).toBe("2026-07-10");
      expect(preset.display.iconKey).toBe(preset.id);
      expect(i18n.exists(preset.display.descriptionKey, { lng: "en" })).toBe(
        true,
      );
      expect(i18n.exists(preset.display.descriptionKey, { lng: "de" })).toBe(
        true,
      );
      expect(new URL(preset.display.setupUrl).protocol).toBe("https:");
      expect(new URL(preset.display.documentationUrl).protocol).toBe("https:");
    }
  });

  it("matches current official discovery and OAuth2 endpoints", () => {
    const [google, github, facebook, x] = AUTH_CONNECTOR_PRESETS;
    expect(google.configuration).toEqual({
      protocol: "oidc",
      issuer: "https://accounts.google.com",
      scopes: ["openid", "profile", "email"],
    });
    expect(github.configuration).toMatchObject({
      protocol: "oauth2",
      authorizationEndpoint: "https://github.com/login/oauth/authorize",
      tokenEndpoint: "https://github.com/login/oauth/access_token",
      userInfoEndpoint: "https://api.github.com/user",
      clientAuthentication: "requestBody",
      claimMappings: { subject: "id", avatarUrl: "avatar_url" },
    });
    expect(facebook.configuration).toMatchObject({
      protocol: "oauth2",
      authorizationEndpoint: "https://www.facebook.com/v25.0/dialog/oauth",
      tokenEndpoint: "https://graph.facebook.com/v25.0/oauth/access_token",
      clientAuthentication: "requestBody",
      claimMappings: { subject: "id", avatarUrl: "picture.data.url" },
    });
    expect(x.configuration).toMatchObject({
      protocol: "oauth2",
      authorizationEndpoint: "https://x.com/i/oauth2/authorize",
      tokenEndpoint: "https://api.x.com/2/oauth2/token",
      clientAuthentication: "basicAuth",
      claimMappings: {
        subject: "data.id",
        email: "data.confirmed_email",
        avatarUrl: "data.profile_image_url",
      },
    });
  });

  it("materializes only generic protocol settings after safe setup validation", () => {
    const draft = createConnectorDraft("github", {
      displayName: "Team sign-in",
      clientId: "client-id",
      clientSecretReference: "secret://identity/team",
      redirectUri: "https://pvlog.example/api/v1/auth/connectors/callback",
    });
    expect(draft.preset).toEqual({ id: "github", revision: 1 });
    expect(draft.configuration.protocol).toBe("oauth2");
    expect(Object.keys(draft.configuration)).not.toEqual(
      expect.arrayContaining(["google", "github", "facebook", "x"]),
    );

    expect(() =>
      createConnectorDraft("x", {
        displayName: "Unsafe callback",
        clientId: "client-id",
        clientSecretReference: "literal secret value",
        redirectUri: "http://pvlog.example/callback",
      }),
    ).toThrow();
    expect(() =>
      createConnectorDraft("x", {
        displayName: "Local development",
        clientId: "client-id",
        clientSecretReference: "secret://identity/local",
        redirectUri: "http://127.0.0.1:8080/callback",
      }),
    ).not.toThrow();
  });
});
