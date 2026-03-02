use std::collections::HashMap;

use anyhow::Result;
use futures_lite::StreamExt;
use iroh::EndpointId;
use iroh_gossip::{
    api::{Event, GossipReceiver, GossipSender},
    proto::TopicId,
};
use tokio::sync::mpsc;

use crate::app::{ChatMessage, UiMessage};
use crate::crypto::decrypt_message;
use crate::protocol::{Message, MessageBody};

// ── Gossip receive loop ───────────────────────────────────────────────────────
pub async fn subscribe_loop(
    mut receiver: GossipReceiver,
    sender: GossipSender,
    topic: TopicId,
    ui_tx: mpsc::Sender<UiMessage>,
    my_id: EndpointId,
    my_name: String,
) -> Result<()> {
    let mut names: HashMap<EndpointId, String> = HashMap::new();
    let mut message_owners: HashMap<u64, EndpointId> = HashMap::new();
    // Messages that arrived before we knew the sender's name.
    let mut pending: Vec<(EndpointId, u64, Vec<u8>, [u8; 12])> = Vec::new();

    names.insert(my_id, my_name.clone());

    while let Some(event) = receiver.try_next().await? {
        if let Event::Received(msg) = event {
            let message = Message::from_bytes(&msg.content)?;
            match message.body {
                MessageBody::AboutMe { from, name } => {
                    let is_new = !names.contains_key(&from);
                    names.insert(from, name.clone());

                    if from != my_id {
                        if is_new {
                            // Re-announce ourselves so the newcomer learns our name.
                            let announce = Message::new(MessageBody::AboutMe {
                                from: my_id,
                                name: my_name.clone(),
                            });
                            let _ = sender.broadcast(announce.to_vec().into()).await;
                        }

                        let _ = ui_tx
                            .send(UiMessage::System(format!("{} joined the chat", name)))
                            .await;

                        // Flush any messages that arrived before we knew this peer's name.
                        pending.retain(|(msg_from, id, ciphertext, nonce)| {
                            if *msg_from != from {
                                return true; // keep — belongs to a different unknown peer
                            }
                            match decrypt_message(ciphertext, nonce, &topic) {
                                Ok(text) => {
                                    let _ = ui_tx.try_send(UiMessage::Chat(ChatMessage {
                                        id: *id,
                                        sender: name.clone(),
                                        content: text,
                                    }));
                                }
                                Err(e) => {
                                    let _ = ui_tx.try_send(UiMessage::System(format!(
                                        "Failed to decrypt message from {}: {}",
                                        name, e
                                    )));
                                }
                            }
                            false // remove from pending after flushing
                        });
                    }
                }

                MessageBody::EncryptedMessage {
                    from,
                    id,
                    ref ciphertext,
                    ref nonce,
                } => {
                    message_owners.insert(id, from);

                    if from == my_id {
                        continue;
                    }

                    // If we don't know this peer's name yet, buffer the message.
                    if !names.contains_key(&from) {
                        pending.push((from, id, ciphertext.clone(), *nonce));
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
                    let authorised = message_owners
                        .get(&id)
                        .map(|owner| *owner == from)
                        .unwrap_or(false);

                    if authorised {
                        message_owners.remove(&id);
                        let _ = ui_tx.send(UiMessage::Delete(id)).await;
                    }
                }
            }
        }
    }
    Ok(())
}
