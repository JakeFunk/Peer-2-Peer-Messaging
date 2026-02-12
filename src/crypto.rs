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
/*
Function:   -get_encryption_key
Purpose:    -Derive a 256-bit (32-byte) symmetric encryption key from a gossip topic ID using SHA-256.
Parameters:
            - &TopicId topic:  Reference to the topic identifier used as the basis for key derivation.

Details:
            - This function generates a deterministic encryption key derived from the provided topic.
            - It initializes a SHA-256 hasher and feeds the topic's raw byte representation into it.
            - The resulting 32-byte hash output is used directly as the symmetric key.
            - The same topic will always produce the same encryption key.
            - This function performs no salting or key stretching beyond a single SHA-256 hash.
            - Returns a 32-byte array suitable for use with ChaCha20Poly1305.
*/
pub fn get_encryption_key(topic: &TopicId) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(topic.as_bytes());
    hasher.finalize().into()
}

/*
Function:   -encrypt_message
Purpose:    -Encrypt a plaintext message using ChaCha20-Poly1305 authenticated encryption.
Parameters:
            - &str text:  The plaintext message to be encrypted.
            - EndpointId from:  Identifier of the sender endpoint.
            - &TopicId topic:  The topic used to derive the symmetric encryption key.
            - u64 id:  A unique identifier for the message.

Details:
            - This function derives a 256-bit encryption key from the provided topic using SHA-256.
            - It initializes a ChaCha20Poly1305 cipher instance with the derived key.
            - A secure random 96-bit nonce is generated using the operating system RNG (OsRng).
            - The plaintext message is encrypted using authenticated encryption (AEAD).
            - The resulting ciphertext includes authentication data to ensure integrity and authenticity.
            - If encryption fails, an error is returned.
            - On success, the function returns a Message struct containing:
                - The sender endpoint ID
                - The message ID
                - The encrypted ciphertext
                - The generated nonce
            - The outer Message struct also includes a randomly generated nonce value.
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
        nonce: rand::random(),
    })
}


/*
Function:   -decrypt_message
Purpose:    -Decrypt a ChaCha20-Poly1305 encrypted message and return the original plaintext string.
Parameters:
            - &[u8] ciphertext:  The encrypted message bytes to be decrypted.
            - &[u8; 12] nonce:  The 96-bit nonce used during encryption.
            - &TopicId topic:  The topic used to derive the symmetric decryption key.

Details:
            - This function derives the same 256-bit encryption key from the topic using SHA-256.
            - It initializes a ChaCha20Poly1305 cipher with the derived key.
            - The provided nonce is converted into a Nonce type required by the cipher.
            - The function attempts authenticated decryption of the ciphertext.
            - If authentication fails (e.g., wrong key, modified ciphertext, or wrong nonce),
              decryption will return an error.
            - If decryption succeeds, the plaintext bytes are converted to a UTF-8 string.
            - If the decrypted bytes are not valid UTF-8, an error is returned.
            - Returns Result<String>, propagating decryption or UTF-8 conversion errors.
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
