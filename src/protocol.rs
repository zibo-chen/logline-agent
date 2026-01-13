//! Logline Protocol (LLP) - Client-side implementation
//!
//! Frame Structure:
//! [Length: u32][Type: u8][Payload: bytes]

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use thiserror::Error;

/// Protocol version
pub const PROTOCOL_VERSION: u8 = 1;

/// Default server port
pub const DEFAULT_PORT: u16 = 12500;

/// Message type identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Handshake = 0x01,
    LogData = 0x02,
    Keepalive = 0xFF,
}

impl TryFrom<u8> for MessageType {
    type Error = ProtocolError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(MessageType::Handshake),
            0x02 => Ok(MessageType::LogData),
            0xFF => Ok(MessageType::Keepalive),
            _ => Err(ProtocolError::UnknownMessageType(value)),
        }
    }
}

#[derive(Error, Debug)]
pub enum ProtocolError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("Unknown message type: {0}")]
    UnknownMessageType(u8),

    #[error("Invalid frame: {0}")]
    InvalidFrame(String),

    #[error("Serialization error: {0}")]
    Serialization(String),
}

/// Handshake message payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakePayload {
    pub project_name: String,
    #[serde(default = "default_version")]
    pub version: u8,
}

fn default_version() -> u8 {
    PROTOCOL_VERSION
}

impl HandshakePayload {
    pub fn new(project_name: impl Into<String>) -> Self {
        Self {
            project_name: project_name.into(),
            version: PROTOCOL_VERSION,
        }
    }
}

/// A protocol frame
#[derive(Debug, Clone)]
pub struct Frame {
    pub message_type: MessageType,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(message_type: MessageType, payload: Vec<u8>) -> Self {
        Self {
            message_type,
            payload,
        }
    }

    /// Create a handshake frame
    pub fn handshake(project_name: impl Into<String>) -> Result<Self, ProtocolError> {
        let payload = HandshakePayload::new(project_name);
        let bytes = serde_json::to_vec(&payload)
            .map_err(|e| ProtocolError::Serialization(e.to_string()))?;
        Ok(Self::new(MessageType::Handshake, bytes))
    }

    /// Create a log data frame
    pub fn log_data(data: Vec<u8>) -> Self {
        Self::new(MessageType::LogData, data)
    }

    /// Create a keepalive frame
    pub fn keepalive() -> Self {
        Self::new(MessageType::Keepalive, Vec::new())
    }

    /// Encode frame to bytes
    pub fn encode(&self) -> Vec<u8> {
        let payload_len = self.payload.len() + 1;
        let mut buf = Vec::with_capacity(4 + payload_len);

        buf.extend_from_slice(&(payload_len as u32).to_be_bytes());
        buf.push(self.message_type as u8);
        buf.extend_from_slice(&self.payload);

        buf
    }

    /// Write frame to writer
    pub fn write_to<W: Write>(&self, writer: &mut W) -> Result<(), ProtocolError> {
        let encoded = self.encode();
        writer.write_all(&encoded)?;
        writer.flush()?;
        Ok(())
    }
}
