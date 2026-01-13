//! Connection management with auto-reconnect
//!
//! Handles TCP connection to Logline server with automatic reconnection.

use crate::protocol::{Frame, ProtocolError};
use anyhow::{Context, Result};
use std::io::BufWriter;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;

/// Connection configuration
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// Server address (host:port)
    pub server_addr: String,
    /// Project name for handshake
    pub project_name: String,
    /// Connection timeout
    pub connect_timeout: Duration,
    /// Initial reconnect delay
    pub initial_reconnect_delay: Duration,
    /// Maximum reconnect delay
    pub max_reconnect_delay: Duration,
}

impl ConnectionConfig {
    pub fn new(server_addr: String, project_name: String) -> Self {
        Self {
            server_addr,
            project_name,
            connect_timeout: Duration::from_secs(10),
            initial_reconnect_delay: Duration::from_secs(1),
            max_reconnect_delay: Duration::from_secs(30),
        }
    }
}

/// Connection state
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Reconnecting { attempt: u32 },
}

/// Manages connection to Logline server
pub struct Connection {
    config: ConnectionConfig,
    stream: Option<BufWriter<TcpStream>>,
    state: ConnectionState,
}

impl Connection {
    pub fn new(config: ConnectionConfig) -> Self {
        Self {
            config,
            stream: None,
            state: ConnectionState::Disconnected,
        }
    }

    /// Try to connect to the server
    pub fn connect(&mut self) -> Result<()> {
        self.state = ConnectionState::Connecting;

        // Resolve address
        let addr = self
            .config
            .server_addr
            .to_socket_addrs()
            .context("Failed to resolve server address")?
            .next()
            .context("No address found")?;

        // Connect with timeout
        let stream = TcpStream::connect_timeout(&addr, self.config.connect_timeout)
            .context("Failed to connect to server")?;

        stream.set_nodelay(true)?;
        stream.set_write_timeout(Some(Duration::from_secs(30)))?;

        let mut writer = BufWriter::new(stream);

        // Send handshake
        let handshake = Frame::handshake(&self.config.project_name)?;
        handshake.write_to(&mut writer)?;

        self.stream = Some(writer);
        self.state = ConnectionState::Connected;

        tracing::info!("Connected to {}", self.config.server_addr);
        Ok(())
    }

    /// Send log data
    pub fn send_data(&mut self, data: Vec<u8>) -> Result<(), ProtocolError> {
        let writer = self.stream.as_mut().ok_or_else(|| {
            ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Not connected",
            ))
        })?;

        let frame = Frame::log_data(data);
        frame.write_to(writer)
    }

    /// Send keepalive
    pub fn send_keepalive(&mut self) -> Result<(), ProtocolError> {
        let writer = self.stream.as_mut().ok_or_else(|| {
            ProtocolError::Io(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Not connected",
            ))
        })?;

        let frame = Frame::keepalive();
        frame.write_to(writer)
    }

    /// Close the connection
    pub fn disconnect(&mut self) {
        self.stream = None;
        self.state = ConnectionState::Disconnected;
    }

    /// Check if connected
    pub fn is_connected(&self) -> bool {
        self.stream.is_some() && self.state == ConnectionState::Connected
    }

    /// Get current state
    pub fn state(&self) -> &ConnectionState {
        &self.state
    }
}

/// Auto-reconnecting connection manager
pub struct ReconnectingConnection {
    config: ConnectionConfig,
}

impl ReconnectingConnection {
    pub fn new(config: ConnectionConfig) -> Self {
        Self { config }
    }

    /// Run the connection loop, receiving data from the channel and sending to server
    pub async fn run(self, mut rx: mpsc::Receiver<Vec<u8>>) -> Result<()> {
        let mut connection = Connection::new(self.config.clone());
        let mut reconnect_delay = self.config.initial_reconnect_delay;
        let mut consecutive_failures = 0u32;
        let mut last_activity = std::time::Instant::now();

        loop {
            // Try to connect if not connected
            if !connection.is_connected() {
                match connection.connect() {
                    Ok(()) => {
                        reconnect_delay = self.config.initial_reconnect_delay;
                        consecutive_failures = 0;
                        tracing::info!("Connection established");
                        last_activity = std::time::Instant::now();
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        connection.state = ConnectionState::Reconnecting {
                            attempt: consecutive_failures,
                        };

                        tracing::warn!(
                            "Connection failed (attempt {}): {}. Retrying in {:?}",
                            consecutive_failures,
                            e,
                            reconnect_delay
                        );

                        sleep(reconnect_delay).await;

                        // Exponential backoff
                        reconnect_delay =
                            std::cmp::min(reconnect_delay * 2, self.config.max_reconnect_delay);

                        continue;
                    }
                }
            }

            // Wait for data with short timeout to stay responsive
            let result = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await;

            match result {
                Ok(Some(data)) => {
                    // Send data
                    let data_len = data.len();
                    if let Err(e) = connection.send_data(data) {
                        tracing::error!("Failed to send data: {}", e);
                        connection.disconnect();
                        continue;
                    }
                    tracing::debug!("Sent {} bytes to server", data_len);
                    last_activity = std::time::Instant::now();
                }
                Ok(None) => {
                    // Channel closed, exit
                    tracing::info!("Data channel closed, shutting down");
                    break;
                }
                Err(_) => {
                    // Timeout - check if we need to send keepalive
                    if last_activity.elapsed() > Duration::from_secs(30) {
                        if let Err(e) = connection.send_keepalive() {
                            tracing::warn!("Keepalive failed: {}", e);
                            connection.disconnect();
                        } else {
                            last_activity = std::time::Instant::now();
                        }
                    }
                }
            }
        }

        Ok(())
    }
}
