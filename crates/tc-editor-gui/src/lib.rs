//! Standalone `std`-only HTTP-served R1 Editor GUI.

pub mod api;
pub mod http;
pub mod json;

use std::io;
use std::net::{SocketAddr, TcpListener};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use api::ApiContext;

/// Running GUI HTTP server.
#[derive(Debug)]
pub struct GuiServer {
    address: SocketAddr,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl GuiServer {
    /// Starts the server on `address` with a workspace directory.
    ///
    /// # Errors
    ///
    /// Returns an I/O error when the listener cannot bind or the workspace
    /// cannot be created.
    pub fn start(address: &str, workspace: &Path) -> io::Result<Self> {
        std::fs::create_dir_all(workspace)?;
        let listener = TcpListener::bind(address)?;
        listener.set_nonblocking(true)?;
        let local = listener.local_addr()?;
        let context = Arc::new(ApiContext::new(workspace));
        let shutdown = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&shutdown);
        let thread = thread::Builder::new()
            .name("r1-gui-http".to_string())
            .spawn(move || {
                while !flag.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let context = Arc::clone(&context);
                            let _ = thread::Builder::new()
                                .name("r1-gui-conn".to_string())
                                .spawn(move || {
                                    if let Ok(Some(request)) = http::read_request(&mut stream) {
                                        let response = api::handle(&context, &request);
                                        let _ = http::write_response(&mut stream, &response);
                                    }
                                });
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            })?;
        Ok(Self {
            address: local,
            shutdown,
            thread: Some(thread),
        })
    }

    /// Returns the bound socket address.
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Returns the base URL.
    #[must_use]
    pub fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    /// Signals shutdown and waits for the accept thread.
    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for GuiServer {
    fn drop(&mut self) {
        self.stop();
    }
}
