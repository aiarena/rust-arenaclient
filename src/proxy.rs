//! Proxy WebSocket receiver

use crate::server::ClientType;
use crossbeam::channel::Sender;
use futures_util::SinkExt;
use futures_util::StreamExt;
use log::info;
use std::net::SocketAddr;
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{Error, Message};
use tokio_tungstenite::{accept_hdr_async_with_config, WebSocketStream};

pub struct HeaderHandler {
    is_supervisor: bool,
}

impl Callback for HeaderHandler {
    fn on_request(
        mut self,
        request: &Request,
        response: Response,
    ) -> Result<Response, ErrorResponse> {
        self.is_supervisor = request.headers().contains_key("supervisor");
        Ok(response)
    }
}

pub struct Client {
    pub(crate) stream: WebSocketStream<TcpStream>,
    addr: SocketAddr,
}

impl Client {
    pub async fn shutdown(&mut self) -> Result<(), Error> {
        self.stream
            .close(Some(CloseFrame {
                code: CloseCode::Normal,
                reason: Default::default(),
            }))
            .await
    }
    pub async fn send_message(&mut self, message: Message) -> Result<(), Error> {
        self.stream.send(message).await
    }
    pub async fn recv_message(&mut self) -> Option<Result<Message, Error>> {
        self.stream.next().await
    }
    pub fn peer_addr(&self) -> &SocketAddr {
        &self.addr
    }
}

/// Accept a new connection
async fn get_connection(server: &mut TcpListener) -> Option<(ClientType, Client)> {
    let mut is_supervisor = false;
    let callback = |req: &Request, response: Response| {
        if req.headers().contains_key("supervisor") {
            is_supervisor = true;
        }
        Ok(response)
    };
    let config = Some(WebSocketConfig {
        max_send_queue: None,
        max_message_size: Some(128 << 20), // 128MiB
        max_frame_size: Some(32 << 20),    // 32MiB
        // This setting allows to accept client frames which are not masked
        // This is not in compliance with RFC 6455 but might be handy in some
        // rare cases where it is necessary to integrate with existing/legacy
        // clients which are sending unmasked frames
        accept_unmasked_frames: true,
    });
    match server.accept().await {
        Ok((stream, peer)) => {
            // let peer = stream.peer_addr().expect("connected streams should have a peer address");
            if let Ok(ws_stream) = accept_hdr_async_with_config(stream, callback, config).await {
                let client = Client {
                    stream: ws_stream,
                    addr: peer,
                };
                return if is_supervisor {
                    Some((ClientType::Controller, client))
                } else {
                    Some((ClientType::Bot, client))
                };
            }
            None
        }
        _ => None,
    }
}

/// Run the proxy server
pub async fn run<A: ToSocketAddrs>(addr: A, channel_out: Sender<(ClientType, Client)>) -> ! {
    let mut server = TcpListener::bind(addr).await.expect("Unable to bind");

    loop {
        if let Some((c_type, client)) = get_connection(&mut server).await {
            info!("Connection accepted: {:?}", client.addr);
            channel_out.send((c_type, client)).expect("Send failed");
        }
    }
}
