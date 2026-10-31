//! The window's side of the approval protocol: the listener, its descriptor and one thread
//! per connection that hands the request to the window's [`ApprovalHandler`] and writes its
//! answer back.

use std::io::{self, BufRead as _, BufReader, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use interprocess::local_socket::traits::{Listener as _, Stream as _, StreamCommon as _};
use interprocess::local_socket::{
    GenericFilePath, ListenerNonblockingMode, ListenerOptions, Stream, ToFsName as _,
};

use super::{
    ApprovalDecision, ApprovalError, ApprovalRequest, ApprovalResponse, MAX_LINE_BYTES,
    PROTOCOL_VERSION, WindowDescriptor, descriptor_path, runtime_dir, socket_path,
};

/// How often the accept loop checks whether it should stop.
const ACCEPT_POLL: Duration = Duration::from_millis(25);

/// How long a client has to send its request line once connected.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// What the window implements to answer approval requests.
///
/// [`ApprovalHandler::request`] runs on the connection's thread and must not block: the
/// window queues the request and answers later through `reply`. Dropping `reply` without an
/// answer closes the connection, which the client takes as no answer.
pub trait ApprovalHandler: Send + Sync + 'static {
    /// A new request; answer it with [`Reply::send`].
    fn request(&self, request: ApprovalRequest, reply: Reply);
}

/// The answer channel of one request.
#[derive(Debug)]
pub struct Reply {
    sender: mpsc::Sender<ApprovalDecision>,
}

impl Reply {
    /// Answers the request with `decision`; `false` when the connection is already gone.
    pub fn send(self, decision: ApprovalDecision) -> bool {
        self.sender.send(decision).is_ok()
    }
}

/// A listening window. Dropping it stops the listener and removes the socket and the
/// descriptor.
#[derive(Debug)]
pub struct ApprovalListener {
    socket: PathBuf,
    descriptor: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ApprovalListener {
    /// The socket it listens on.
    #[must_use]
    pub fn socket(&self) -> &Path {
        &self.socket
    }

    /// The descriptor it advertises the socket with.
    #[must_use]
    pub fn descriptor(&self) -> &Path {
        &self.descriptor
    }
}

impl Drop for ApprovalListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take()
            && thread.join().is_err()
        {
            log::warn!(target: "reqflux_engine", "the approval listener thread panicked");
        }
        for path in [&self.descriptor, &self.socket] {
            if let Err(error) = std::fs::remove_file(path)
                && error.kind() != io::ErrorKind::NotFound
            {
                log::warn!(
                    target: "reqflux_engine",
                    "could not remove {}: {error}",
                    path.display()
                );
            }
        }
    }
}

/// Listens for approvals in [`runtime_dir`] as this process.
///
/// # Errors
///
/// No runtime dir, an unsupported platform, or the directory, socket or descriptor could
/// not be created.
pub fn serve(handler: Arc<dyn ApprovalHandler>) -> Result<ApprovalListener, ApprovalError> {
    let dir = runtime_dir().ok_or(ApprovalError::NoRuntimeDir)?;
    serve_in(&dir, std::process::id(), handler)
}

/// Listens for approvals in `dir` as the process `pid`: creates `dir` owner-only, the socket
/// `approvals-<pid>.sock` and the descriptor `approvals-<pid>.json` in it, both owner-only.
///
/// # Errors
///
/// An unsupported platform, or the directory, socket or descriptor could not be created.
#[cfg(unix)]
pub fn serve_in(
    dir: &Path,
    pid: u32,
    handler: Arc<dyn ApprovalHandler>,
) -> Result<ApprovalListener, ApprovalError> {
    use std::os::unix::fs::{DirBuilderExt as _, MetadataExt as _, PermissionsExt as _};

    let at = |path: &Path| {
        let path = path.to_path_buf();
        move |source| ApprovalError::Io { path, source }
    };
    if !dir.is_dir() {
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)
            .map_err(at(dir))?;
    }
    let metadata = std::fs::symlink_metadata(dir).map_err(at(dir))?;
    if !metadata.is_dir() {
        return Err(ApprovalError::Io {
            path: dir.to_path_buf(),
            source: io::Error::other("not a directory"),
        });
    }
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(at(dir))?;
    let owner = metadata.uid();

    let socket = socket_path(dir, pid);
    let name = socket
        .as_path()
        .to_fs_name::<GenericFilePath>()
        .map_err(at(&socket))?;
    let listener = ListenerOptions::new()
        .name(name)
        .try_overwrite(true)
        .nonblocking(ListenerNonblockingMode::Accept)
        .create_sync()
        .map_err(at(&socket))?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
        .map_err(at(&socket))?;

    let descriptor = descriptor_path(dir, pid);
    write_descriptor(
        &descriptor,
        &WindowDescriptor {
            version: PROTOCOL_VERSION,
            pid,
            socket: socket.clone(),
        },
    )
    .map_err(at(&descriptor))?;

    let stop = Arc::new(AtomicBool::new(false));
    let stopped = Arc::clone(&stop);
    let thread = std::thread::Builder::new()
        .name("reqflux-approvals".to_owned())
        .spawn(move || accept_loop(&listener, &stopped, owner, &handler))
        .map_err(at(&socket))?;
    Ok(ApprovalListener {
        socket,
        descriptor,
        stop,
        thread: Some(thread),
    })
}

/// Listens for approvals: unsupported on this platform.
///
/// # Errors
///
/// Always [`ApprovalError::Unsupported`].
#[cfg(not(unix))]
pub fn serve_in(
    dir: &Path,
    pid: u32,
    handler: Arc<dyn ApprovalHandler>,
) -> Result<ApprovalListener, ApprovalError> {
    drop((dir, pid, handler));
    Err(ApprovalError::Unsupported)
}

/// Writes `descriptor` to `path`, owner-only.
#[cfg(unix)]
fn write_descriptor(path: &Path, descriptor: &WindowDescriptor) -> io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt as _;

    let text = serde_json::to_string(descriptor).map_err(io::Error::other)?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())?;
    file.sync_data()
}

/// Accepts connections until `stop`, each on its own thread.
#[cfg(unix)]
fn accept_loop(
    listener: &interprocess::local_socket::Listener,
    stop: &AtomicBool,
    owner: u32,
    handler: &Arc<dyn ApprovalHandler>,
) {
    while !stop.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok(stream) => {
                let handler = Arc::clone(handler);
                let spawned = std::thread::Builder::new()
                    .name("reqflux-approval".to_owned())
                    .spawn(move || {
                        if let Err(error) = connection(stream, owner, handler.as_ref()) {
                            log::debug!(target: "reqflux_engine", "approval connection: {error}");
                        }
                    });
                if let Err(error) = spawned {
                    log::warn!(target: "reqflux_engine", "cannot serve an approval: {error}");
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(ACCEPT_POLL);
            }
            Err(error) => {
                log::warn!(target: "reqflux_engine", "approval listener: {error}");
                std::thread::sleep(ACCEPT_POLL);
            }
        }
    }
}

/// Serves one connection: checks the peer is `owner`, reads the request line, hands it to
/// `handler` and writes its answer. A malformed request or one of another version closes
/// the connection unanswered.
#[cfg(unix)]
fn connection(stream: Stream, owner: u32, handler: &dyn ApprovalHandler) -> io::Result<()> {
    let peer = stream.peer_creds()?.euid();
    if peer != Some(owner) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refused a connection from another user",
        ));
    }
    stream.set_recv_timeout(Some(REQUEST_TIMEOUT))?;
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    (&mut reader).take(MAX_LINE_BYTES).read_line(&mut line)?;
    let request: ApprovalRequest = serde_json::from_str(line.trim_end())
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    if request.version != PROTOCOL_VERSION {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "protocol version {} is not {PROTOCOL_VERSION}",
                request.version
            ),
        ));
    }
    let id = request.id;
    let (sender, receiver) = mpsc::channel();
    handler.request(request, Reply { sender });
    let Ok(decision) = receiver.recv() else {
        return Ok(());
    };
    let response = ApprovalResponse {
        version: PROTOCOL_VERSION,
        id,
        decision,
    };
    let mut text = serde_json::to_string(&response).map_err(io::Error::other)?;
    text.push('\n');
    let stream = reader.get_mut();
    stream.write_all(text.as_bytes())?;
    stream.flush()
}
