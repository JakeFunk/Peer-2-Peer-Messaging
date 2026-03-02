mod app;
mod crypto;
mod gossip;
mod protocol;
mod tui;

use std::str::FromStr;

use anyhow::Result;
use clap::Parser;
use iroh::{protocol::Router, Endpoint};
use iroh_gossip::net::Gossip;
use tokio::sync::mpsc;

use app::UiMessage;
use crypto::encrypt_message;
use protocol::{Message, MessageBody, Ticket};

#[derive(Parser, Debug)]
struct Args {
    #[clap(short, long)]
    name: Option<String>,
    #[clap(short, long, default_value = "0")]
    bind_port: u16,
    #[clap(subcommand)]
    command: Command,
}

#[derive(Parser, Debug)]
enum Command {
    Open,
    Join
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let (topic, endpoints) = match &args.command {
        Command::Open => {
            let topic = iroh_gossip::proto::TopicId::from_bytes(rand::random());
            (topic, vec![])
        }
        Command::Join => {
            println!("Paste your ticket and press Enter:");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let ticket_str = input.trim();
            let Ticket { topic, endpoints } = Ticket::from_str(ticket_str)?;
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
  
    match &args.command {
        Command::Open => {
            println!("╔══════════════════════════════════════════════════════════════╗");
            println!("║                    ENCRYPTED CHAT ROOM                       ║");
            println!("╚══════════════════════════════════════════════════════════════╝");
            println!();
            println!("Share this ticket with others to join:");
            println!("{}", ticket);
            println!();
        }
        Command::Join => {
            println!("╔══════════════════════════════════════════════════════════════╗");
            println!("║                    ENCRYPTED CHAT ROOM                       ║");
            println!("╚══════════════════════════════════════════════════════════════╝");
            println!();
        }
    }


    let (ui_tx, ui_rx) = mpsc::channel::<UiMessage>(100);
    let (input_tx, mut input_rx) = mpsc::channel::<(String, u64)>(100);
    let (delete_tx, mut delete_rx) = mpsc::channel::<u64>(32);

    let endpoint_ids = endpoints.iter().map(|p| p.id).collect();

    let (sender, receiver) = gossip
        .subscribe_and_join(topic, endpoint_ids)
        .await?
        .split();

    let my_name = args.name.clone().unwrap_or_else(|| "Anonymous".to_string());
    let my_id = endpoint.id();

    // Broadcast our name immediately.
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
        sender.clone(),
        topic,
        ui_tx_clone,
        my_id,
        my_name.clone(),
    ));

    // Spawn message sender / deleter loop.
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

    // Run the TUI — opens immediately, peers appear as they connect.
    tui::run_tui(ui_rx, input_tx, delete_tx).await?;

    router.shutdown().await?;
    std::process::exit(0);

}
