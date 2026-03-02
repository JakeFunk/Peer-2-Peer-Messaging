use anyhow::Result;
use chacha20poly1305::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    ChaCha20Poly1305, Key, Nonce,
};
use hkdf::Hkdf;
use iroh::EndpointId;
use iroh_gossip::proto::TopicId;
use sha2::Sha256;

use crate::protocol::{Message, MessageBody};

// ── Encryption helpers ──────────────────────────────────────────────────────────

/// Application-specific salt for HKDF.
/// Public and fixed — exists purely for domain separation, not secrecy.
const HKDF_SALT: &[u8] = b"encrypted-chat-v1-salt";

/// HKDF info string binding the derived key to its purpose.
/// Changing this string produces a completely different key from the same topic.
const HKDF_INFO: &[u8] = b"encrypted-chat/message-key/v1";

/* Function: -get_encryption_key
   Purpose:
   -Derive a 256-bit symmetric encryption key from a gossip topic ID using
    HKDF-SHA256 (RFC 5869).
   Parameters:
   - &TopicId topic: Reference to the topic identifier used as input key material.
   Details:
   - The topic acts as the IKM (input key material). Security depends on keeping
     the ticket private — anyone who intercepts the ticket can derive this key.
     There is no forward secrecy; this is acceptable for a gossip-based system
     where the topic is the shared secret.
   - The fixed salt provides domain separation from bare SHA-256 and binds the
     key to this application.
   - The info string ("encrypted-chat/message-key/v1") ensures keys derived here
     cannot be confused with keys derived for any other purpose.
   - To derive additional keys in future (e.g. for auth or key rotation), call
     hk.expand() with a different info string on the same Hkdf instance.
   - Returns a 32-byte array suitable for use with ChaCha20Poly1305.
*/
pub fn get_encryption_key(topic: &TopicId) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(Some(HKDF_SALT), topic.as_bytes());
    let mut okm = [0u8; 32];
    hk.expand(HKDF_INFO, &mut okm)
        .expect("32 bytes is a valid HKDF-SHA256 output length");
    okm
}

/* Function: -encrypt_message
   Purpose:
   -Encrypt a plaintext message using ChaCha20-Poly1305 authenticated encryption.
   Parameters:
   - &str text: The plaintext message to be encrypted.
   - EndpointId from: Identifier of the sender endpoint.
   - &TopicId topic: The topic used to derive the symmetric encryption key.
   - u64 id: A unique identifier for the message.
   Details:
   - Derives a 256-bit encryption key from the topic via HKDF-SHA256.
   - A secure random 96-bit nonce is generated per message using OsRng.
   - The plaintext is encrypted with AEAD — ciphertext includes an
     authentication tag ensuring integrity and authenticity.
   - Returns a Message struct containing the sender ID, message ID,
     ciphertext, and nonce.
   - Returns Result<Message>, propagating encryption errors if they occur.
*/
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
    })
}

/* Function: -decrypt_message
   Purpose:
   -Decrypt a ChaCha20-Poly1305 encrypted message and return the plaintext string.
   Parameters:
   - &[u8] ciphertext: The encrypted message bytes to be decrypted.
   - &[u8; 12] nonce: The 96-bit nonce used during encryption.
   - &TopicId topic: The topic used to derive the symmetric decryption key.
   Details:
   - Derives the same 256-bit key from the topic via HKDF-SHA256.
   - Authenticated decryption — fails explicitly if the key, nonce, or
     ciphertext have been tampered with.
   - Decrypted bytes are validated as UTF-8 before being returned.
   - Returns Result<String>, propagating decryption or UTF-8 errors.
*/
pub fn decrypt_message(ciphertext: &[u8], nonce: &[u8; 12], topic: &TopicId) -> Result<String> {
    let key = get_encryption_key(topic);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let nonce_obj = Nonce::from_slice(nonce);
    let plaintext = cipher
        .decrypt(nonce_obj, ciphertext)
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

    String::from_utf8(plaintext).map_err(Into::into)
}
