use anyhow::Result;
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305, Key, Nonce,
};
use iroh::EndpointId;
use iroh_gossip::proto::TopicId;
use sha2::{Digest, Sha256};

use crate::protocol::{Message, MessageBody};

// ── Encryption helpers ────────────────────────────────────────────────────────

pub fn get_encryption_key(topic: &TopicId) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(topic.as_bytes());
    hasher.finalize().into()
}

pub fn encrypt_message(text: &str, from: EndpointId, topic: &TopicId, id: u64) -> Result<Message> {
    let key = get_encryption_key(topic);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let nonce_bytes = ChaCha20Poly1305::generate_nonce(&mut OsRng);

    let ciphertext = cipher
        .encrypt(&nonce_bytes, text.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    Ok(Message {
        body: MessageBody::EncryptedMessage {
            from,
            id,
            ciphertext,
            nonce: nonce_bytes.into(),
        },
        nonce: rand::random(),
    })
}

pub fn decrypt_message(ciphertext: &[u8], nonce: &[u8; 12], topic: &TopicId) -> Result<String> {
    let key = get_encryption_key(topic);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let nonce_obj = Nonce::from_slice(nonce);

    let plaintext = cipher
        .decrypt(nonce_obj, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

    String::from_utf8(plaintext).map_err(Into::into)
}
