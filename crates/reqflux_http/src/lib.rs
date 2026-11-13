//! Sending fully resolved HTTP requests: TLS, client certificates, proxies, cookies,
//! redirects, decompression, timeouts and cancellation. Inputs and outputs carry no ReqFlux model types; `reqflux_engine` builds a
//! [`PreparedRequest`] and reads a [`Response`].

pub mod auth;
mod client;
mod error;
mod network;
mod redirect;
mod request;
mod response;
mod timing;
mod wire;

pub use client::{HttpClient, RedirectCheck, RedirectRules, SecretMask};
pub use error::HttpError;
pub use network::{
    CertificateFile, ClientCertificate, CookieJar, ManualProxy, NetworkSettings, ProxySetting,
};
pub use request::{MultipartPart, MultipartValue, PreparedBody, PreparedRequest, RequestOptions};
pub use response::Response;
pub use timing::{HopTimings, Timings};
pub use wire::WireRequest;
