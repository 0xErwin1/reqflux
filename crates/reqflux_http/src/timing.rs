//! The timing breakdown of a send (v1-features S4): DNS, connect, first byte and download
//! for every hop of a redirect chain.
//!
//! `reqwest` exposes no timing events, so the client measures them through the two hooks it
//! has: a DNS resolver ([`TimedResolver`]) and a connector layer ([`TimingLayer`]) around the
//! connector that opens a connection. Both report to the [`Recorder`] of the hop in flight,
//! found through a Tokio task-local, so concurrent sends on one client never mix their
//! numbers. The connector layer wraps the whole connector, TCP and TLS together, so the TLS
//! handshake cannot be told apart from the TCP connect: [`HopTimings::tls`] stays `None` and
//! [`HopTimings::connect`] includes the handshake for HTTPS.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use reqwest::dns::{Addrs, Name, Resolve, Resolving};

use crate::wire::WireRequest;

/// The timing breakdown of a send, one entry per request sent: the redirects followed in
/// order, then the final request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Timings {
    /// One entry per hop, the final one last; empty when nothing was measured.
    pub hops: Vec<HopTimings>,
}

impl Timings {
    /// The timings of the final request, the one whose response came back.
    #[must_use]
    pub fn last(&self) -> Option<&HopTimings> {
        self.hops.last()
    }
}

/// The phases of one request of a send.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HopTimings {
    /// The request this hop put on the wire.
    pub wire: WireRequest,
    /// The DNS lookup; `None` when there was none: a reused connection or an IP address.
    pub dns: Option<Duration>,
    /// Opening the connection after the lookup: TCP, plus the TLS handshake for HTTPS;
    /// `None` when a pooled connection was reused.
    pub connect: Option<Duration>,
    /// The TLS handshake alone. Always `None` with the current transport, which reports
    /// the handshake inside `connect`.
    pub tls: Option<Duration>,
    /// From the connection being ready to the response headers: sending the request and
    /// waiting for the server.
    pub first_byte: Duration,
    /// Reading the whole body; zero for a redirect, whose body is not read.
    pub download: Duration,
}

/// What the hooks measured for the hop in flight.
#[derive(Debug, Default)]
pub(crate) struct Recorder {
    dns: Mutex<Option<Duration>>,
    connect: Mutex<Option<Duration>>,
}

impl Recorder {
    /// The DNS time and the whole connector time (DNS included) measured so far.
    pub(crate) fn measured(&self) -> (Option<Duration>, Option<Duration>) {
        let dns = *self.dns.lock().unwrap_or_else(PoisonError::into_inner);
        let connect = *self.connect.lock().unwrap_or_else(PoisonError::into_inner);
        (dns, connect)
    }

    fn set_dns(&self, elapsed: Duration) {
        *self.dns.lock().unwrap_or_else(PoisonError::into_inner) = Some(elapsed);
    }

    fn set_connect(&self, elapsed: Duration) {
        *self.connect.lock().unwrap_or_else(PoisonError::into_inner) = Some(elapsed);
    }
}

tokio::task_local! {
    static RECORDER: Arc<Recorder>;
}

/// Runs `future` (one hop's send) with `recorder` receiving what the hooks measure.
pub(crate) async fn recorded<F: Future>(recorder: Arc<Recorder>, future: F) -> F::Output {
    RECORDER.scope(recorder, future).await
}

/// The recorder of the hop being polled, if any.
fn current() -> Option<Arc<Recorder>> {
    RECORDER.try_with(Arc::clone).ok()
}

/// The phases of a hop from what was measured: `connector` is the whole connector time,
/// DNS included, and `headers` the time from the send to the response headers.
pub(crate) fn hop(recorder: &Recorder, headers: Duration) -> HopTimings {
    let (dns, connector) = recorder.measured();
    HopTimings {
        wire: WireRequest::default(),
        dns,
        connect: connector.map(|connector| connector.saturating_sub(dns.unwrap_or_default())),
        tls: None,
        first_byte: headers.saturating_sub(connector.unwrap_or_default()),
        download: Duration::ZERO,
    }
}

/// The system resolver (`getaddrinfo` through Tokio), timing each lookup.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TimedResolver;

impl Resolve for TimedResolver {
    fn resolve(&self, name: Name) -> Resolving {
        let recorder = current();
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let started = Instant::now();
            let addresses: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            if let Some(recorder) = recorder {
                recorder.set_dns(started.elapsed());
            }
            let addresses: Addrs = Box::new(addresses.into_iter());
            Ok(addresses)
        })
    }
}

/// A connector layer timing every connection the client opens.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TimingLayer;

impl<S> tower_layer::Layer<S> for TimingLayer {
    type Service = TimedConnector<S>;

    fn layer(&self, inner: S) -> Self::Service {
        TimedConnector { inner }
    }
}

/// The connector of [`TimingLayer`].
#[derive(Debug, Clone)]
pub(crate) struct TimedConnector<S> {
    inner: S,
}

impl<S, R> tower_service::Service<R> for TimedConnector<S>
where
    S: tower_service::Service<R>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, context: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(context)
    }

    fn call(&mut self, request: R) -> Self::Future {
        let recorder = current();
        let started = Instant::now();
        let connecting = self.inner.call(request);
        Box::pin(async move {
            let connection = connecting.await;
            if let (Some(recorder), Ok(_)) = (recorder, &connection) {
                recorder.set_connect(started.elapsed());
            }
            connection
        })
    }
}
