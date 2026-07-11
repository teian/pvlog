use async_trait::async_trait;
use pvlog_application::{
    PortError, ResolvedWebhookResponse, SafeWebhookSender, WebhookDnsResolver,
    WebhookHttpTransport, WebhookNetworkPolicy, WebhookRequest, WebhookSecurityError,
    WebhookSender,
};
use std::{
    collections::{HashMap, VecDeque},
    error::Error,
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    sync::{Arc, Mutex},
};
use url::Url;

#[tokio::test]
async fn blocks_insecure_private_and_rebound_destinations() -> Result<(), Box<dyn Error>> {
    for (endpoint, address) in [
        (
            "http://public.example/hook",
            IpAddr::V4(Ipv4Addr::new(203, 0, 113, 10)),
        ),
        (
            "https://private.example/hook",
            IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)),
        ),
        ("https://ipv6.example/hook", IpAddr::V6(Ipv6Addr::LOCALHOST)),
    ] {
        let sender = sender(
            HashMap::from([(host(endpoint)?, vec![address])]),
            vec![ok()],
        );
        assert!(matches!(
            sender.send(request(endpoint)?).await,
            Err(PortError::Rejected(_))
        ));
    }
    Ok(())
}

#[tokio::test]
async fn re_resolves_redirects_and_enforces_redirect_body_and_timeout_limits()
-> Result<(), Box<dyn Error>> {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let limits = Arc::new(Mutex::new(Vec::new()));
    let resolver = FakeResolver {
        addresses: HashMap::from([
            (
                "first.example".to_owned(),
                vec![IpAddr::V4(Ipv4Addr::new(203, 0, 113, 1))],
            ),
            (
                "second.example".to_owned(),
                vec![IpAddr::V4(Ipv4Addr::new(203, 0, 113, 2))],
            ),
        ]),
        calls: calls.clone(),
    };
    let transport = FakeTransport {
        responses: Mutex::new(VecDeque::from([
            ResolvedWebhookResponse {
                status: 302,
                body_bytes: 0,
                redirect_location: Some(Url::parse("https://second.example/final")?),
                retry_after_seconds: None,
            },
            ok(),
        ])),
        limits: limits.clone(),
    };
    let policy = WebhookNetworkPolicy {
        maximum_redirects: 1,
        timeout_milliseconds: 1_234,
        maximum_response_bytes: 42,
        ..WebhookNetworkPolicy::default()
    };
    let sender = SafeWebhookSender::new(resolver, transport, policy);
    assert_eq!(
        sender
            .send(request("https://first.example/hook")?)
            .await?
            .status,
        204
    );
    assert_eq!(
        calls.lock().map_err(|_| "resolver lock")?.as_slice(),
        ["first.example", "second.example"]
    );
    assert_eq!(
        limits.lock().map_err(|_| "transport lock")?.as_slice(),
        [(1_234, 42), (1_234, 42)]
    );
    Ok(())
}

fn sender(
    addresses: HashMap<String, Vec<IpAddr>>,
    responses: Vec<ResolvedWebhookResponse>,
) -> SafeWebhookSender<FakeResolver, FakeTransport> {
    SafeWebhookSender::new(
        FakeResolver {
            addresses,
            calls: Arc::new(Mutex::new(Vec::new())),
        },
        FakeTransport {
            responses: Mutex::new(responses.into()),
            limits: Arc::new(Mutex::new(Vec::new())),
        },
        WebhookNetworkPolicy::default(),
    )
}
fn request(endpoint: &str) -> Result<WebhookRequest, url::ParseError> {
    Ok(WebhookRequest {
        endpoint: Url::parse(endpoint)?,
        headers: Vec::new(),
        body: b"{}".to_vec(),
    })
}
fn host(endpoint: &str) -> Result<String, Box<dyn Error>> {
    Ok(Url::parse(endpoint)?
        .host_str()
        .ok_or("missing host")?
        .to_owned())
}
fn ok() -> ResolvedWebhookResponse {
    ResolvedWebhookResponse {
        status: 204,
        body_bytes: 0,
        redirect_location: None,
        retry_after_seconds: None,
    }
}

struct FakeResolver {
    addresses: HashMap<String, Vec<IpAddr>>,
    calls: Arc<Mutex<Vec<String>>>,
}
#[async_trait]
impl WebhookDnsResolver for FakeResolver {
    async fn resolve(&self, host: &str) -> Result<Vec<IpAddr>, WebhookSecurityError> {
        self.calls
            .lock()
            .map_err(|_| WebhookSecurityError::DnsFailure)?
            .push(host.to_owned());
        self.addresses
            .get(host)
            .cloned()
            .ok_or(WebhookSecurityError::DnsFailure)
    }
}
struct FakeTransport {
    responses: Mutex<VecDeque<ResolvedWebhookResponse>>,
    limits: Arc<Mutex<Vec<(u32, usize)>>>,
}
#[async_trait]
impl WebhookHttpTransport for FakeTransport {
    async fn send_to(
        &self,
        _request: &WebhookRequest,
        _resolved_address: IpAddr,
        timeout_milliseconds: u32,
        maximum_response_bytes: usize,
    ) -> Result<ResolvedWebhookResponse, WebhookSecurityError> {
        self.limits
            .lock()
            .map_err(|_| WebhookSecurityError::Transport)?
            .push((timeout_milliseconds, maximum_response_bytes));
        self.responses
            .lock()
            .map_err(|_| WebhookSecurityError::Transport)?
            .pop_front()
            .ok_or(WebhookSecurityError::Transport)
    }
}
