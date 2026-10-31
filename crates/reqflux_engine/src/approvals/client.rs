//! The asking side of one exchange: connect to a window's socket, send one request line and
//! read one response line.

use std::io::{self, BufRead as _, BufReader, Read as _, Write as _};
use std::path::Path;
use std::time::Duration;

use interprocess::local_socket::traits::Stream as _;
use interprocess::local_socket::{GenericFilePath, Stream, ToFsName as _};

use super::{ApprovalRequest, ApprovalResponse, MAX_LINE_BYTES, PROTOCOL_VERSION};

/// Why an exchange produced no decision.
#[derive(Debug, thiserror::Error)]
pub enum AskError {
    /// The socket refused the connection or the request could not be written: the window
    /// is gone.
    #[error("cannot reach the window: {0}")]
    Unreachable(#[source] io::Error),
    /// The window did not answer within the limit, closed the connection, or answered
    /// something that is not a response to this request.
    #[error("no answer from the window: {0}")]
    NoAnswer(String),
}

/// Sends `request` to the window listening on `socket` and waits up to `limit` for its
/// response: the decision and, for a denial, the reason the user gave.
///
/// # Errors
///
/// The window is unreachable, or gave no valid answer in time.
pub fn ask(
    socket: &Path,
    request: &ApprovalRequest,
    limit: Duration,
) -> Result<ApprovalResponse, AskError> {
    let name = socket
        .to_fs_name::<GenericFilePath>()
        .map_err(AskError::Unreachable)?;
    let mut stream = Stream::connect(name).map_err(AskError::Unreachable)?;
    let mut line = serde_json::to_string(request)
        .map_err(|error| AskError::Unreachable(io::Error::other(error)))?;
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .and_then(|()| stream.flush())
        .map_err(AskError::Unreachable)?;
    stream
        .set_recv_timeout(Some(limit.max(Duration::from_millis(1))))
        .map_err(AskError::Unreachable)?;
    let mut reader = BufReader::new(stream);
    let mut answer = String::new();
    let read = (&mut reader)
        .take(MAX_LINE_BYTES)
        .read_line(&mut answer)
        .map_err(|error| AskError::NoAnswer(error.to_string()))?;
    if read == 0 {
        return Err(AskError::NoAnswer("the connection was closed".to_owned()));
    }
    let response: ApprovalResponse = serde_json::from_str(answer.trim_end())
        .map_err(|_| AskError::NoAnswer("the answer is not a response".to_owned()))?;
    if response.version != PROTOCOL_VERSION || response.id != request.id {
        return Err(AskError::NoAnswer(
            "the answer is not a response to this request".to_owned(),
        ));
    }
    Ok(response)
}
