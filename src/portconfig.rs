//! Full port configuration

use portpicker::pick_unused_port;

use protobuf::RepeatedField;
use sc2_proto::sc2api::{PortSet, RequestJoinGame};

/// Full set of ports needed by SC2
#[derive(Debug, Clone)]
pub struct PortConfig {
    shared: u16,
    server_game: u16,
    server_base: u16,
    client_game: u16,
    client_base: u16,
}
impl PortConfig {
    /// Create a set of random ports
    pub fn new() -> Option<Self> {
        Some(Self {
            shared: pick_unused_port()?,
            server_game: pick_unused_port()?,
            server_base: pick_unused_port()?,
            client_game: pick_unused_port()?,
            client_base: pick_unused_port()?,
        })
    }

    /// Apply port config to a game join request
    pub fn apply_proto(&self, req: &mut RequestJoinGame, singleplayer: bool) {
        req.set_shared_port(self.shared as i32);

        if !singleplayer {
            let mut server_ps = PortSet::new();
            server_ps.set_game_port(self.server_game as i32);
            server_ps.set_base_port(self.server_base as i32);
            req.set_server_ports(server_ps);

            let mut client_ps = PortSet::new();
            client_ps.set_game_port(self.client_game as i32);
            client_ps.set_base_port(self.client_base as i32);
            req.set_client_ports(RepeatedField::from_vec(vec![client_ps]));
        }
    }
}
