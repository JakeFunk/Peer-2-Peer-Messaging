use std::collections::HashMap;

use anyhow::Result;
use futures_lite::StreamExt;
use iroh::EndpointId;
use iroh_gossip::{
    api::{Event, GossipReceiver},
    proto::TopicId,
};
use tokio::sync::mpsc;

use crate::app::{ChatMessage, UiMessage};
use crate::crypto::decrypt_message;
use crate::protocol::{Message, MessageBody};

// ── Gossip receive loop ───────────────────────────────────────────────────────

pub async fn subscribe_loop(
    mut receiver: GossipReceiver,
    topic: TopicId,
    ui_tx: mpsc::Sender<UiMessage>,
    my_id: EndpointId,
    my_name: String,
) -> Result<()> {
    // Maps EndpointId → display name so we can attribute messages correctly.
    // Also records which EndpointId sent which message ID, so we only honour
    // delete requests from the original sender.
    let mut names: HashMap<EndpointId, String> = HashMap::new();
    let mut message_owners: HashMap<u64, EndpointId> = HashMap::new();

    names.insert(my_id, my_name);

    while let Some(event) = receiver.try_next().await? {
        if let Event::Received(msg) = event {
            let message = Message::from_bytes(&msg.content)?;

            match message.body {
                MessageBody::AboutMe { from, name } => {
                    names.insert(from, name.clone());
                    if from != my_id {
                        let _ = ui_tx
                            .send(UiMessage::System(format!("{} joined the chat", name)))
                            .await;
                    }
                }

                MessageBody::EncryptedMessage {
                    from,
                    id,
                    ref ciphertext,
                    ref nonce,
                } => {
                    // Record ownership so delete requests can be validated.
                    message_owners.insert(id, from);

                    // Skip our own messages – already shown when sent.
                    if from == my_id {
                        continue;
                    }

                    let name = names
                        .get(&from)
                        .cloned()
                        .unwrap_or_else(|| from.fmt_short().to_string());

                    match decrypt_message(ciphertext, nonce, &topic) {
                        Ok(text) => {
                            let _ = ui_tx
                                .send(UiMessage::Chat(ChatMessage {
                                    id,
                                    sender: name,
                                    content: text,
                                    encrypted: true,
                                }))
                                .await;
                        }
                        Err(e) => {
                            let _ = ui_tx
                                .send(UiMessage::System(format!(
                                    "Failed to decrypt message from {}: {}",
                                    name, e
                                )))
                                .await;
                        }
                    }
                }

                MessageBody::DeleteMessage { from, id } => {
                    // Only honour the delete if it came from the original sender.
                    let authorised = message_owners
                        .get(&id)
                        .map(|owner| *owner == from)
                        .unwrap_or(false);

                    if authorised {
                        message_owners.remove(&id);
                        let _ = ui_tx.send(UiMessage::Delete(id)).await;
                    }
                    // If not authorised, silently ignore.
                }
            }
        }
    }
    Ok(())
}
