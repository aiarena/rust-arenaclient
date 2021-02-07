//! Proxy WebSocket receiver

use crossbeam::channel::Sender;
use std::net::ToSocketAddrs;

use crate::result::SC2Result;
use crate::server::ClientType;
use log::{error, info};
use websocket::client::sync::Client as GenericClient;
use websocket::server::sync::Server as GenericServer;
use websocket::server::NoTlsAcceptor;
use websocket::stream::sync::TcpStream;

/// Server socket
pub type Server = GenericServer<NoTlsAcceptor>;
/// Client socket
pub type Client = GenericClient<TcpStream>;

/// Accept a new connection
fn get_connection(server: &mut Server) -> SC2Result<(ClientType, Client)> {
    match server.accept() {
        Ok(t) => match t.request.headers.get_raw("supervisor") {
            Some(_) => match t.accept() {
                Ok(e) => Ok((ClientType::Controller, e)),
                Err((_, e)) => Err(e.to_string()),
            },
            None => match t.accept() {
                Ok(e) => Ok((ClientType::Bot, e)),
                Err((_, e)) => Err(e.to_string()),
            },
        },
        Err(e) => Err(e.error.to_string()),
    }
}

/// Run the proxy server
pub fn run<A: ToSocketAddrs>(addr: A, channel_out: Sender<(ClientType, Client)>) -> ! {
    let mut server = Server::bind(addr).expect("Unable to bind");

    loop {
        match get_connection(&mut server) {
            Ok((c_type, conn)) => {
                info!("Connection accepted: {:?}", conn.peer_addr().unwrap(),);
                channel_out.send((c_type, conn)).expect("Send failed");
            }
            Err(e) => error!("{}", e),
        }
    }
}
