use std::{fmt, str::FromStr};

use anyhow::Result;
use iroh::{EndpointAddr, EndpointId};
use iroh_gossip::proto::TopicId;
use serde::{Deserialize, Serialize};

// ── Wire protocol ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub body: MessageBody,
    pub nonce: [u8; 16],
}

#[derive(Debug, Serialize, Deserialize)]
pub enum MessageBody {
    AboutMe {
        from: EndpointId,
        name: String,
    },
    /// Encrypted chat message.
    EncryptedMessage {
        from: EndpointId,
        /// Unique message ID, stored outside the ciphertext so peers can
        /// reference it for deletion without decrypting first.
        id: u64,
        ciphertext: Vec<u8>,
        nonce: [u8; 12],
    },
    /// Cooperative delete request – all peers should remove the message with
    /// this ID from their display. Only honored when `from` matches the
    /// original sender.
    DeleteMessage {
        from: EndpointId,
        id: u64,
    },
}

impl Message {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(Into::into)
    }

    pub fn new(body: MessageBody) -> Self {
        Self {
            body,
            nonce: rand::random(),
        }
    }

    pub fn to_vec(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("serde_json::to_vec is infallible")
    }
}

// ── Ticket ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct Ticket {
    pub topic: TopicId,
    pub endpoints: Vec<EndpointAddr>,
}

impl Ticket {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(Into::into)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("serde_json::to_vec is infallible")
    }
}

impl fmt::Display for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut text = data_encoding::BASE32_NOPAD.encode(&self.to_bytes()[..]);
        text.make_ascii_lowercase();
        write!(f, "{}", text)
    }
}

impl FromStr for Ticket {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = data_encoding::BASE32_NOPAD.decode(s.to_ascii_uppercase().as_bytes())?;
        Self::from_bytes(&bytes)
    }
}
