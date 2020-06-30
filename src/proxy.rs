//! Proxy WebSocket receiver

use crossbeam::channel::Sender;
use std::net::ToSocketAddrs;

use crate::server::ClientType;
use websocket::client::sync::Client as GenericClient;
use websocket::server::sync::Server as GenericServer;
use websocket::server::NoTlsAcceptor;
use websocket::stream::sync::TcpStream;

/// Server socket
pub type Server = GenericServer<NoTlsAcceptor>;
/// Client socket
pub type Client = GenericClient<TcpStream>;

/// Accept a new connection
fn get_connection(server: &mut Server) -> Option<(ClientType, Client)> {
    match server.accept() {
        Ok(t) => match t.request.headers.get_raw("supervisor") {
            Some(_) => match t.accept() {
                Ok(e) => Some((ClientType::Controller, e)),
                _ => None,
            },
            None => match t.accept() {
                Ok(e) => Some((ClientType::Bot, e)),
                _ => None,
            },
        },
        _ => None,
    }
}

/// Run the proxy server
pub fn run<A: ToSocketAddrs>(addr: A, channel_out: Sender<(ClientType, Client)>) -> ! {
    let mut server = Server::bind(addr).expect("Unable to bind");

    loop {
        if let Some((c_type, conn)) = get_connection(&mut server) {
            println!("Connection accepted: {:?}", conn.peer_addr().unwrap());
            channel_out.send((c_type, conn)).expect("Send failed");
        }
    }
}
