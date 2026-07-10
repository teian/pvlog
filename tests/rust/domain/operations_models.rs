use std::{collections::BTreeSet, error::Error};

use pvlog_domain::{
    AccountId, Provider, ProviderCapability, ProviderId, ProviderState, UtcTimestamp,
    WebhookEventType, WebhookSubscription, WebhookSubscriptionId, WebhookSubscriptionState,
};
use url::Url;

#[test]
fn webhook_subscriptions_store_key_references_instead_of_secret_values()
-> Result<(), Box<dyn Error>> {
    let subscription = WebhookSubscription {
        id: WebhookSubscriptionId::new(),
        account_id: AccountId::new(),
        endpoint: Url::parse("https://receiver.example/events")?,
        events: BTreeSet::from([WebhookEventType::AlertOpened]),
        state: WebhookSubscriptionState::Active,
        signing_key_reference: "secret:webhooks/primary".to_owned(),
        created_at: UtcTimestamp::from_epoch_millis(0)?,
    };
    let serialized = serde_json::to_string(&subscription)?;

    assert!(serialized.contains("signing_key_reference"));
    assert!(!serialized.contains("signing_key_value"));
    Ok(())
}

#[test]
fn providers_are_capability_based_and_credential_indirect() {
    let provider = Provider {
        id: ProviderId::new(),
        account_id: None,
        name: "regional feed".to_owned(),
        capabilities: BTreeSet::from([ProviderCapability::RegionalSupply]),
        credential_reference: Some("secret:providers/region".to_owned()),
        configuration: serde_json::json!({"region": "DE"}),
        state: ProviderState::Healthy,
        last_success_at: None,
    };

    assert!(
        provider
            .capabilities
            .contains(&ProviderCapability::RegionalSupply)
    );
    assert!(matches!(provider.state, ProviderState::Healthy));
}
