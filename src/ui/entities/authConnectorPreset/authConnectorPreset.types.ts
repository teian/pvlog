/** Stable identifier of an administrator-facing connector preset. */
export type AuthConnectorPresetId = "facebook" | "github" | "google" | "x";

/** Display-only metadata. It is never sent as protocol configuration. */
export interface ConnectorPresetDisplay {
  /** Provider name shown to administrators. */
  name: string;
  /** Translation key for the setup summary. */
  descriptionKey: string;
  /** Local icon registry key resolved by the presentation layer. */
  iconKey: AuthConnectorPresetId;
  /** Provider console where an administrator creates credentials. */
  setupUrl: string;
  /** Current authoritative protocol documentation. */
  documentationUrl: string;
}

/** Generic normalized claim paths understood by the OAuth2 connector. */
export interface ConnectorClaimMappings {
  /** Immutable provider subject path. */
  subject: string;
  /** Display-name path when supplied by the provider. */
  displayName?: string;
  /** Email path when supplied by the provider. */
  email?: string;
  /** Verified-email boolean path when supplied by the provider. */
  emailVerified?: string;
  /** Avatar URL path when supplied by the provider. */
  avatarUrl?: string;
}

/** Generic OpenID Connect settings populated by a preset. */
export interface OidcPresetConfiguration {
  /** Protocol discriminator. */
  protocol: "oidc";
  /** Discovery issuer URL. */
  issuer: string;
  /** Least-privilege identity scopes. */
  scopes: readonly string[];
}

/** Generic OAuth2 Authorization Code settings populated by a preset. */
export interface OAuth2PresetConfiguration {
  /** Protocol discriminator. */
  protocol: "oauth2";
  /** Authorization endpoint URL. */
  authorizationEndpoint: string;
  /** Token endpoint URL. */
  tokenEndpoint: string;
  /** Bearer-authenticated normalized user-info endpoint URL. */
  userInfoEndpoint: string;
  /** Standard client authentication method for the token endpoint. */
  clientAuthentication: "basicAuth" | "requestBody";
  /** Least-privilege identity scopes. */
  scopes: readonly string[];
  /** Provider-response paths mapped into the generic identity model. */
  claimMappings: ConnectorClaimMappings;
}

/** Protocol settings supported by the connector administration API. */
export type ConnectorPresetConfiguration =
  OAuth2PresetConfiguration | OidcPresetConfiguration;

/** Versioned provider setup template consumed only by administrator UI/configuration code. */
export interface AuthConnectorPreset {
  /** Stable catalog identifier. */
  id: AuthConnectorPresetId;
  /** Revision of this individual definition. */
  revision: number;
  /** Date on which official documentation was last checked. */
  verifiedAt: string;
  /** Presentation-only provider metadata. */
  display: ConnectorPresetDisplay;
  /** Provider-neutral settings materialized by the preset. */
  configuration: ConnectorPresetConfiguration;
}

/** Administrator-supplied values shared by every preset. */
export interface ConnectorSetupInput {
  /** Instance-specific connector name. */
  displayName: string;
  /** Client identifier issued by the provider. */
  clientId: string;
  /** Secret-manager reference, never the secret value. */
  clientSecretReference: string;
  /** Exact callback URL registered at the provider. */
  redirectUri: string;
}

/** Generic connector draft ready for the protocol-neutral administration endpoint. */
export interface ConnectorDraft extends ConnectorSetupInput {
  /** Preset provenance for future update diagnostics. */
  preset: { id: AuthConnectorPresetId; revision: number };
  /** Generic protocol configuration without display metadata. */
  configuration: ConnectorPresetConfiguration;
}
