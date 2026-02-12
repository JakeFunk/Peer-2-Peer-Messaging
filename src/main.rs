mod app;
mod crypto;
mod gossip;
mod protocol;
mod tui;

use std::str::FromStr;

use anyhow::Result;
use clap::Parser;
use futures_lite::StreamExt;
use iroh::{protocol::Router, Endpoint};
use iroh_gossip::{api::Event, net::Gossip};
use tokio::sync::mpsc;

use app::UiMessage;
use crypto::encrypt_message;
use protocol::{Message, MessageBody, Ticket};

#[derive(Parser, Debug)]
struct Args {
    /// Set your nickname.
    #[clap(short, long)]
    name: Option<String>,
    /// Set the bind port for our socket. By default, a random port will be used.
    #[clap(short, long, default_value = "0")]
    bind_port: u16,
    #[clap(subcommand)]
    command: Command,
}

#[derive(Parser, Debug)]
enum Command {
    Open,
    Join { ticket: String },
}

// ── Entry point ───────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let (topic, endpoints) = match &args.command {
        Command::Open => {
            let topic = iroh_gossip::proto::TopicId::from_bytes(rand::random());
            (topic, vec![])
        }
        Command::Join { ticket } => {
            let Ticket { topic, endpoints } = Ticket::from_str(ticket)?;
            (topic, endpoints)
        }
    };

    let endpoint = Endpoint::bind().await?;
    let gossip = Gossip::builder().spawn(endpoint.clone());
    let router = Router::builder(endpoint.clone())
        .accept(iroh_gossip::ALPN, gossip.clone())
        .spawn();

    let ticket = {
        let me = endpoint.addr();
        let endpoints = vec![me];
        Ticket { topic, endpoints }
    };

    // Print ticket to terminal BEFORE TUI launches.
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║                    ENCRYPTED CHAT ROOM                       ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();
    println!("Topic: {}", topic);
    println!();
    println!("Share this ticket with others to join:");
    println!("{}", ticket);
    println!();

    // Setup channels.
    let (ui_tx, ui_rx) = mpsc::channel::<UiMessage>(100);
    // (message text, pre-assigned id) so the sender loop can embed the same ID
    // that we already recorded locally.
    let (input_tx, mut input_rx) = mpsc::channel::<(String, u64)>(100);
    // Channel for delete requests: sends the message ID to delete everywhere.
    let (delete_tx, mut delete_rx) = mpsc::channel::<u64>(32);

    // Join the gossip topic.
    let endpoint_ids = endpoints.iter().map(|p| p.id).collect();
    if endpoints.is_empty() {
        println!("Waiting for someone to join...");
        println!("   Press Ctrl+C to cancel");
    } else {
        println!("Connecting to {} peers...", endpoints.len());
    }

    let (sender, mut receiver) = gossip
        .subscribe_and_join(topic, endpoint_ids)
        .await?
        .split();

    // Wait for first peer to connect before launching TUI.
    if endpoints.is_empty() {
        println!("Waiting for first peer connection...");

        let mut temp_receiver = receiver;
        let mut first_peer_connected = false;

        while !first_peer_connected {
            if let Some(event) = temp_receiver.try_next().await? {
                if let Event::Received(msg) = event {
                    if let Ok(message) = Message::from_bytes(&msg.content) {
                        if matches!(message.body, MessageBody::AboutMe { .. }) {
                            first_peer_connected = true;
                            println!("Peer connected! Launching chat interface...");
                            std::thread::sleep(std::time::Duration::from_millis(500));
                        }
                    }
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        receiver = temp_receiver;
    } else {
        println!("Connected!");
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    // Broadcast our name.
    let my_name = args.name.clone().unwrap_or_else(|| "Anonymous".to_string());
    let my_id = endpoint.id();

    let message = Message::new(MessageBody::AboutMe {
        from: my_id,
        name: my_name.clone(),
    });
    sender.broadcast(message.to_vec().into()).await?;
    ui_tx
        .send(UiMessage::System(format!("You joined as {}", my_name)))
        .await?;
    ui_tx
        .send(UiMessage::System(
            "INSERT mode – type & Enter to send. ESC for NORMAL mode.".to_string(),
        ))
        .await?;

    // Spawn gossip receiver loop.
    let ui_tx_clone = ui_tx.clone();
    tokio::spawn(gossip::subscribe_loop(
        receiver,
        topic,
        ui_tx_clone,
        my_id,
        my_name.clone(),
    ));

    // Spawn message sender / deleter loop.
    // We clone `sender` for the delete path by wrapping both in a single task
    // and using tokio::select! to drive whichever channel fires first.
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some((text, id)) = input_rx.recv() => {
                    if let Ok(msg) = encrypt_message(&text, my_id, &topic, id) {
                        let _ = sender.broadcast(msg.to_vec().into()).await;
                    }
                }
                Some(id) = delete_rx.recv() => {
                    let msg = Message::new(MessageBody::DeleteMessage { from: my_id, id });
                    let _ = sender.broadcast(msg.to_vec().into()).await;
                }
                else => break,
            }
        }
    });

    // Run the TUI.
    tui::run_tui(ui_rx, input_tx, delete_tx).await?;

    router.shutdown().await?;
    Ok(())
}
