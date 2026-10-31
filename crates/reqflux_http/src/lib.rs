//! Sending fully resolved HTTP requests: TLS, redirects, decompression, timeouts and
//! cancellation. Inputs and outputs carry no ReqFlux model types; `reqflux_engine` builds a
//! [`PreparedRequest`] and reads a [`Response`].

mod client;
mod error;
mod request;
mod response;

pub use client::{HttpClient, RedirectCheck};
pub use error::HttpError;
pub use request::{MultipartPart, MultipartValue, PreparedBody, PreparedRequest, RequestOptions};
pub use response::Response;
