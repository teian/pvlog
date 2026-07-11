//! SSRF-safe webhook delivery with connection-time DNS policy enforcement.

use crate::{PortError, WebhookRequest, WebhookResponse, WebhookSender};
use async_trait::async_trait;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use thiserror::Error;
use url::Url;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebhookNetworkPolicy {
    pub require_https: bool,
    pub allow_private_networks: bool,
    pub maximum_redirects: u8,
    pub timeout_milliseconds: u32,
    pub maximum_response_bytes: usize,
}
impl Default for WebhookNetworkPolicy {
    fn default() -> Self {
        Self {
            require_https: true,
            allow_private_networks: false,
            maximum_redirects: 0,
            timeout_milliseconds: 5_000,
            maximum_response_bytes: 64 * 1024,
        }
    }
}

#[async_trait]
pub trait WebhookDnsResolver: Send + Sync {
    async fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, WebhookSecurityError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedWebhookResponse {
    pub status: u16,
    pub body_bytes: usize,
    pub redirect_location: Option<Url>,
    pub retry_after_seconds: Option<u32>,
}

#[async_trait]
pub trait WebhookHttpTransport: Send + Sync {
    async fn send_to(
        &self,
        request: &WebhookRequest,
        resolved_address: IpAddr,
        timeout_milliseconds: u32,
        maximum_response_bytes: usize,
    ) -> Result<ResolvedWebhookResponse, WebhookSecurityError>;
}

pub struct SafeWebhookSender<R, T> {
    resolver: R,
    transport: T,
    policy: WebhookNetworkPolicy,
}
impl<R, T> SafeWebhookSender<R, T> {
    #[must_use]
    pub const fn new(resolver: R, transport: T, policy: WebhookNetworkPolicy) -> Self {
        Self {
            resolver,
            transport,
            policy,
        }
    }
}

#[async_trait]
impl<R: WebhookDnsResolver, T: WebhookHttpTransport> WebhookSender for SafeWebhookSender<R, T> {
    async fn send(&self, mut request: WebhookRequest) -> Result<WebhookResponse, PortError> {
        for redirects in 0..=self.policy.maximum_redirects {
            validate_url(&request.endpoint, self.policy).map_err(port)?;
            let host = request
                .endpoint
                .host_str()
                .ok_or_else(|| port(WebhookSecurityError::MissingHost))?;
            let addresses = self.resolver.resolve(host).await.map_err(port)?;
            let address = validated_address(&addresses, self.policy).map_err(port)?;
            let response = self
                .transport
                .send_to(
                    &request,
                    address,
                    self.policy.timeout_milliseconds,
                    self.policy.maximum_response_bytes,
                )
                .await
                .map_err(port)?;
            if response.body_bytes > self.policy.maximum_response_bytes {
                return Err(port(WebhookSecurityError::ResponseTooLarge));
            }
            if let Some(location) = response.redirect_location {
                if redirects == self.policy.maximum_redirects {
                    return Err(port(WebhookSecurityError::RedirectLimit));
                }
                request.endpoint = location;
                continue;
            }
            return Ok(WebhookResponse {
                status: response.status,
                retry_after_seconds: response.retry_after_seconds,
            });
        }
        Err(port(WebhookSecurityError::RedirectLimit))
    }
}

fn validate_url(endpoint: &Url, policy: WebhookNetworkPolicy) -> Result<(), WebhookSecurityError> {
    if policy.require_https && endpoint.scheme() != "https" {
        return Err(WebhookSecurityError::HttpsRequired);
    }
    if !matches!(endpoint.scheme(), "https" | "http") {
        return Err(WebhookSecurityError::UnsupportedScheme);
    }
    if !endpoint.username().is_empty() || endpoint.password().is_some() {
        return Err(WebhookSecurityError::UserInfoForbidden);
    }
    Ok(())
}
fn validated_address(
    addresses: &[IpAddr],
    policy: WebhookNetworkPolicy,
) -> Result<IpAddr, WebhookSecurityError> {
    if addresses.is_empty() {
        return Err(WebhookSecurityError::DnsEmpty);
    }
    if !policy.allow_private_networks && addresses.iter().any(|address| blocked(*address)) {
        return Err(WebhookSecurityError::AddressBlocked);
    }
    addresses
        .first()
        .copied()
        .ok_or(WebhookSecurityError::DnsEmpty)
}
fn blocked(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(value) => blocked_v4(value),
        IpAddr::V6(value) => blocked_v6(value),
    }
}
fn blocked_v4(value: Ipv4Addr) -> bool {
    value.is_private()
        || value.is_loopback()
        || value.is_link_local()
        || value.is_multicast()
        || value.is_broadcast()
        || value.is_unspecified()
        || value.octets()[0] == 0
        || value.octets()[0] >= 224
        || value.octets()[0..2] == [100, 64]
        || value.octets()[0..2] == [169, 254]
}
fn blocked_v6(value: Ipv6Addr) -> bool {
    value.is_loopback()
        || value.is_unspecified()
        || value.is_multicast()
        || (value.segments()[0] & 0xfe00) == 0xfc00
        || (value.segments()[0] & 0xffc0) == 0xfe80
        || value.to_ipv4_mapped().is_some_and(blocked_v4)
}
fn port(error: WebhookSecurityError) -> PortError {
    PortError::Rejected(error.to_string())
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum WebhookSecurityError {
    #[error("webhook endpoint must use HTTPS")]
    HttpsRequired,
    #[error("webhook endpoint scheme is unsupported")]
    UnsupportedScheme,
    #[error("webhook endpoint user info is forbidden")]
    UserInfoForbidden,
    #[error("webhook endpoint has no host")]
    MissingHost,
    #[error("webhook DNS resolution returned no addresses")]
    DnsEmpty,
    #[error("webhook address is blocked by network policy")]
    AddressBlocked,
    #[error("webhook redirect limit exceeded")]
    RedirectLimit,
    #[error("webhook response body exceeds the configured limit")]
    ResponseTooLarge,
    #[error("webhook DNS resolution failed")]
    DnsFailure,
    #[error("webhook transport failed")]
    Transport,
}
