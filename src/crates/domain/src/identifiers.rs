use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! identifier {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Generates a time-sortable `UUIDv7` identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            /// Wraps an already validated UUID.
            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            /// Returns the underlying UUID.
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value).map(Self)
            }
        }
    };
}

identifier!(UserId, "Stable local user identifier.");
identifier!(AccountId, "Stable tenant account identifier.");
identifier!(
    ConnectorId,
    "Stable external authentication connector identifier."
);
identifier!(
    ExternalIdentityId,
    "Stable link between a local user and an external identity."
);
identifier!(SessionId, "Stable interactive session identifier.");
identifier!(RoleId, "Stable authorization role identifier.");
identifier!(
    RoleAssignmentId,
    "Stable authorization assignment identifier."
);
identifier!(MembershipId, "Stable account membership identifier.");
identifier!(ApiCredentialId, "Stable API credential identifier.");
identifier!(AuditEventId, "Stable audit event identifier.");
identifier!(EquipmentId, "Stable system equipment identifier.");
identifier!(TariffId, "Stable effective-dated tariff identifier.");
identifier!(ChannelId, "Stable extended measurement channel identifier.");
identifier!(CorrectionId, "Stable observation correction identifier.");
identifier!(SegmentId, "Stable archived telemetry segment identifier.");
identifier!(TeamId, "Stable community team identifier.");
identifier!(TeamMembershipId, "Stable team membership identifier.");
identifier!(FavouriteId, "Stable user favourite identifier.");
identifier!(AlertRuleId, "Stable alert rule identifier.");
identifier!(AlertEventId, "Stable emitted alert event identifier.");
identifier!(
    WebhookSubscriptionId,
    "Stable webhook subscription identifier."
);
identifier!(WebhookDeliveryId, "Stable webhook delivery identifier.");
identifier!(
    ProviderId,
    "Stable external data provider configuration identifier."
);
identifier!(JobId, "Stable background job identifier.");
identifier!(ImportId, "Stable import workflow identifier.");
identifier!(ExportId, "Stable export workflow identifier.");
identifier!(
    SystemId,
    "Globally routable photovoltaic system identifier."
);
identifier!(ObservationId, "Stable canonical observation identifier.");
identifier!(
    RequestId,
    "Stable request and audit-correlation identifier."
);
