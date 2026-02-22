//! RetroArch network command client (UDP).

use std::net::UdpSocket;

use log::{debug, warn};

pub struct RetroArchClient {
    socket: UdpSocket,
    addr: String,
}

impl RetroArchClient {
    pub fn new(host: &str, port: u16) -> Result<Self, String> {
        let socket = UdpSocket::bind("0.0.0.0:0").map_err(|e| format!("udp bind: {e}"))?;
        socket
            .set_nonblocking(true)
            .map_err(|e| format!("set nonblocking: {e}"))?;
        let addr = format!("{host}:{port}");
        Ok(RetroArchClient { socket, addr })
    }

    pub fn send_command(&self, command: &str) -> bool {
        debug!("Sending RA command: {command} -> {}", self.addr);
        match self.socket.send_to(command.as_bytes(), &self.addr) {
            Ok(_) => true,
            Err(e) => {
                warn!("Failed to send RA command '{command}': {e}");
                false
            }
        }
    }
}
