use async_trait::async_trait;
use pvlog_application::{
    AuthorizationBoundary, AuthorizationBoundaryError, AuthorizationBoundaryPorts,
    AuthorizedAccountRoute, PortError, ProtectedAccountRequest,
};
use pvlog_domain::{AccountId, Permission, PrincipalId, RequestId, UserId};
use std::{
    error::Error,
    sync::{Arc, Mutex},
};

#[tokio::test]
async fn authorization_always_precedes_routing_and_both_outcomes_are_audited()
-> Result<(), Box<dyn Error>> {
    for allowed in [false, true] {
        let ports = Arc::new(FakePorts {
            allowed,
            events: Mutex::new(Vec::new()),
        });
        let boundary = AuthorizationBoundary::new(ports.clone());
        let account_id = AccountId::new();
        let result = boundary
            .authorize_and_route(&ProtectedAccountRequest {
                principal: PrincipalId::User(UserId::new()),
                account_id,
                system_id: None,
                permission: Permission::AccountRead,
                request_id: RequestId::new(),
                action: "account.read",
            })
            .await;
        let events = ports.events.lock().map_err(|_| "poisoned")?.clone();
        if allowed {
            assert!(result.is_ok());
            assert_eq!(events, ["authorize", "route", "succeeded"]);
        } else {
            assert!(matches!(result, Err(AuthorizationBoundaryError::Forbidden)));
            assert_eq!(events, ["authorize", "denied"]);
        }
    }
    Ok(())
}

struct FakePorts {
    allowed: bool,
    events: Mutex<Vec<&'static str>>,
}
#[async_trait]
impl AuthorizationBoundaryPorts for FakePorts {
    async fn is_authorized(&self, _request: &ProtectedAccountRequest) -> Result<bool, PortError> {
        self.events
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .push("authorize");
        Ok(self.allowed)
    }
    async fn account_route(
        &self,
        account_id: AccountId,
    ) -> Result<Option<AuthorizedAccountRoute>, PortError> {
        self.events
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .push("route");
        Ok(Some(AuthorizedAccountRoute {
            account_id,
            opaque_route: "opaque".to_owned(),
        }))
    }
    async fn append_audit(
        &self,
        _request: &ProtectedAccountRequest,
        outcome: &'static str,
    ) -> Result<(), PortError> {
        self.events
            .lock()
            .map_err(|_| PortError::Unavailable)?
            .push(outcome);
        Ok(())
    }
}
