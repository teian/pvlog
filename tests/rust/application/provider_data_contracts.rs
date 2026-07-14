use pvlog_application::{
    CircuitBreaker, CircuitBreakerPolicy, CircuitState, ExternalDataConfiguration,
    ExternalDataKind, ExternalDataLicense, ProviderConfigurationError,
};
use pvlog_domain::{ProviderId, UtcTimestamp};
use std::error::Error;
use url::Url;

#[test]
fn administrator_configuration_requires_explicit_adapter_cache_and_license()
-> Result<(), Box<dyn Error>> {
    let mut configuration = configuration()?;
    configuration.validate()?;
    assert!(!configuration.license.redistribution_permitted);

    configuration.license.attribution.clear();
    assert_eq!(
        configuration.validate(),
        Err(ProviderConfigurationError::MissingLicenseMetadata)
    );
    Ok(())
}

#[test]
fn circuit_opens_at_threshold_probes_after_timeout_and_recovers() -> Result<(), Box<dyn Error>> {
    let mut circuit = CircuitBreaker::new(CircuitBreakerPolicy {
        failure_threshold: 2,
        recovery_timeout_milliseconds: 5_000,
    });
    let start = UtcTimestamp::from_epoch_millis(10_000)?;
    circuit.record_failure(start);
    assert_eq!(circuit.state(), CircuitState::Closed);
    circuit.record_failure(start);
    assert_eq!(
        circuit.state(),
        CircuitState::Opened {
            retry_at_epoch_millis: 15_000
        }
    );
    assert!(!circuit.allow(UtcTimestamp::from_epoch_millis(14_999)?));
    assert!(circuit.allow(UtcTimestamp::from_epoch_millis(15_000)?));
    assert_eq!(circuit.state(), CircuitState::HalfOpen);
    circuit.record_success();
    assert_eq!(circuit.state(), CircuitState::Closed);
    Ok(())
}

fn configuration() -> Result<ExternalDataConfiguration, url::ParseError> {
    Ok(ExternalDataConfiguration {
        provider_id: ProviderId::new(),
        kind: ExternalDataKind::Insolation,
        adapter: "administrator_http_json".to_owned(),
        endpoint: Url::parse("https://data.example.test/insolation")?,
        credential_secret_reference: Some("secret:providers/solar".to_owned()),
        request_timeout_milliseconds: 2_000,
        cache_ttl_seconds: 900,
        maximum_stale_seconds: 3_600,
        license: ExternalDataLicense {
            identifier: "operator-supplied".to_owned(),
            attribution: "Example data owner".to_owned(),
            source_url: Url::parse("https://data.example.test/license")?,
            redistribution_permitted: false,
        },
        enabled: true,
    })
}
