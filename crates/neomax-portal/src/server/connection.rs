use std::net::{TcpListener, TcpStream};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use serde_json::json;

use crate::http::HttpResponse;
use crate::security::log_internal;

use super::PortalServer;

const MAX_CONNECTIONS: usize = 32;
const IO_TIMEOUT: Duration = Duration::from_secs(15);

impl<S, E, P> PortalServer<S, E, P>
where
    S: crate::source::PortalSource + 'static,
    E: crate::actions::LocalActionExecutor + 'static,
    P: crate::actions::PrStateResolver + 'static,
{
    pub fn run(self) -> Result<()> {
        let listener = TcpListener::bind(self.bind.socket_addr())?;
        println!(
            "neomax-portal -> {}  (Ctrl-C to stop)",
            crate::address::local_url(self.bind)
        );
        let server = Arc::new(self);
        let active = Arc::new(AtomicUsize::new(0));
        for connection in listener.incoming() {
            match connection {
                Ok(mut stream) => {
                    if active
                        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
                            (current < MAX_CONNECTIONS).then_some(current + 1)
                        })
                        .is_err()
                    {
                        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                        let _ =
                            write_rejection(&mut stream, 503, "portal connection limit reached");
                        continue;
                    }
                    let server = Arc::clone(&server);
                    let active = Arc::clone(&active);
                    thread::spawn(move || {
                        let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
                        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
                        if let Err(error) = server.handle(&mut stream) {
                            log_internal("request failed", &error);
                            let _ =
                                HttpResponse::json(500, &json!({"error": "portal request failed"}))
                                    .and_then(|response| response.write_to(&mut stream));
                        }
                        active.fetch_sub(1, Ordering::AcqRel);
                    });
                }
                Err(error) => eprintln!("portal connection failed: {error}"),
            }
        }
        Ok(())
    }

    pub fn handle(&self, stream: &mut TcpStream) -> Result<()> {
        let request = match crate::http::read_request(stream) {
            Ok(request) => request,
            Err(error) => {
                log_internal("invalid HTTP request", &error);
                HttpResponse::json(400, &json!({"error": "invalid portal request"}))?
                    .write_to(stream)?;
                return Ok(());
            }
        };
        let response = match self.response(&request) {
            Ok(response) => response,
            Err(error) => {
                log_internal("request response failed", &error);
                HttpResponse::json(500, &json!({"error": "portal request failed"}))?
            }
        };
        response.write_to(stream)
    }
}

fn write_rejection(stream: &mut TcpStream, status: u16, message: &str) -> Result<()> {
    HttpResponse::json(status, &json!({"error": message}))?.write_to(stream)
}
