use async_trait::async_trait;
use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use pvlog_compatibility::{
    LegacyAuth, LegacyNotificationCallback, LegacyNotificationError,
    LegacyNotificationRegistration, LegacyNotificationUseCases, legacy_notification_callback_body,
    legacy_notification_router,
};
use serde::Deserialize;
use std::{
    error::Error,
    sync::{Arc, Mutex},
};
use tower::ServiceExt as _;

#[derive(Deserialize)]
struct Golden {
    register: Case,
    deregister: Case,
    limit: Case,
}
#[derive(Deserialize)]
struct Case {
    path: String,
    status: u16,
    body: String,
}

#[tokio::test]
async fn notification_routes_enforce_registration_and_preserve_callback_shape()
-> Result<(), Box<dyn Error>> {
    let service = Arc::new(FakeNotifications::default());
    let app = legacy_notification_router(service.clone());
    let golden: Golden = serde_json::from_str(include_str!(
        "../fixtures/pvoutput/notifications-golden.json"
    ))?;
    for case in [golden.register, golden.deregister, golden.limit] {
        assert_case(&app, &case).await?;
    }
    assert_eq!(
        service
            .registered
            .lock()
            .map_err(|_| "registered lock")?
            .as_slice(),
        [6]
    );
    assert_eq!(
        legacy_notification_callback_body(&LegacyNotificationCallback {
            application_id: "my.application.id".to_owned(),
            message: "Idle for 15 minutes & counting".to_owned(),
            alert_type: 6
        }),
        "appid=my.application.id&msg=Idle+for+15+minutes+%26+counting&type=6"
    );
    Ok(())
}

#[tokio::test]
async fn every_documented_alert_type_is_accepted() -> Result<(), Box<dyn Error>> {
    let app = legacy_notification_router(Arc::new(FakeNotifications::default()));
    for alert_type in [0, 1, 3, 4, 5, 6, 8, 11, 14, 15, 16, 17, 18, 19, 20, 23, 24] {
        let case = Case {
            path: format!(
                "/service/r2/registernotification.jsp?key=write-key&sid=42&appid=app&url=https%3A%2F%2Fapp.example%2Fcallback&type={alert_type}"
            ),
            status: 200,
            body: "Registered Notification".to_owned(),
        };
        assert_case(&app, &case).await?;
    }
    Ok(())
}
async fn assert_case(app: &axum::Router, case: &Case) -> Result<(), Box<dyn Error>> {
    let response = app
        .clone()
        .oneshot(Request::builder().uri(&case.path).body(Body::empty())?)
        .await?;
    let status = response.status();
    let body = to_bytes(response.into_body(), 1024 * 1024).await?;
    assert_eq!(status, StatusCode::from_u16(case.status)?);
    assert_eq!(std::str::from_utf8(&body)?, case.body);
    Ok(())
}

#[derive(Default)]
struct FakeNotifications {
    registered: Mutex<Vec<u8>>,
}
#[async_trait]
impl LegacyNotificationUseCases for FakeNotifications {
    async fn register(
        &self,
        auth: &LegacyAuth,
        registration: LegacyNotificationRegistration,
    ) -> Result<(), LegacyNotificationError> {
        authorize(auth)?;
        if registration.application_id == "limit" {
            return Err(LegacyNotificationError::RegistrationLimit);
        }
        self.registered
            .lock()
            .map_err(|_| LegacyNotificationError::Unavailable)?
            .push(registration.alert_type);
        Ok(())
    }
    async fn deregister(
        &self,
        auth: &LegacyAuth,
        application_id: &str,
        alert_type: u8,
    ) -> Result<(), LegacyNotificationError> {
        authorize(auth)?;
        assert_eq!(application_id, "my.application.id");
        assert_eq!(alert_type, 6);
        Ok(())
    }
}
fn authorize(auth: &LegacyAuth) -> Result<(), LegacyNotificationError> {
    if auth.api_key == "write-key" && auth.system_id == 42 {
        Ok(())
    } else {
        Err(LegacyNotificationError::Unauthorized)
    }
}
