use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::thread;

use super::super::PortalServer;
use super::fixtures::{EmptySource, FailingPrState};
use crate::address::LocalBind;

#[test]
fn handler_redacts_internal_errors_for_clients() {
    let server = PortalServer::new(LocalBind::loopback(8787), EmptySource)
        .with_pr_state_resolver(FailingPrState);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let client = thread::spawn(move || {
        let mut stream = TcpStream::connect(address).unwrap();
        stream
            .write_all(
                b"GET /api/prstate?url=https%3A%2F%2Fgithub.com%2Fowner%2Frepo%2Fpull%2F1 HTTP/1.1\r\nHost: localhost:8787\r\n\r\n",
            )
            .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    });
    let (mut stream, _) = listener.accept().unwrap();
    server.handle(&mut stream).unwrap();
    drop(stream);
    let response = client.join().unwrap();
    assert!(response.contains("500 Internal Server Error"));
    assert!(response.contains("portal request failed"));
    assert!(!response.contains("/fixture/private"));
    assert!(!response.contains("secret"));
}
