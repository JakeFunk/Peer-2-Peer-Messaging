// ── UI types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Unique ID used for cooperative deletion across peers.
    pub id: u64,
    pub sender: String,
    pub content: String,
    pub encrypted: bool,
}

#[derive(Debug, Clone)]
pub enum UiMessage {
    Chat(ChatMessage),
    System(String),
    /// Instructs the UI to remove the chat message with this ID.
    Delete(u64),
}

// ── Modal editing ─────────────────────────────────────────────────────────────

#[derive(PartialEq)]
pub enum Mode {
    /// Typing mode – keys go into the input buffer.
    Insert,
    /// Command mode – Ctrl+C quits, Ctrl+D deletes last message everywhere.
    Normal,
}

// ── App state ─────────────────────────────────────────────────────────────────

pub struct App {
    pub input: String,
    pub messages: Vec<UiMessage>,
    pub mode: Mode,
    /// Tracks the IDs of messages *we* sent, oldest-first, so we can delete
    /// the most recent one with Ctrl+D.
    pub my_sent_ids: Vec<u64>,
    /// How many lines from the bottom we are scrolled. 0 = pinned to bottom.
    pub scroll_offset: usize,
}

impl App {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            messages: Vec::new(),
            mode: Mode::Insert,
            my_sent_ids: Vec::new(),
            scroll_offset: 0,
        }
    }

    pub fn add_message(&mut self, msg: UiMessage) {
        // Apply deletions immediately.
        if let UiMessage::Delete(id) = &msg {
            let id = *id;
            self.messages.retain(|m| match m {
                UiMessage::Chat(c) => c.id != id,
                _ => true,
            });
            self.my_sent_ids.retain(|&i| i != id);
            self.messages
                .push(UiMessage::System("A message was deleted.".to_string()));
            return;
        }

        self.messages.push(msg);
        // Keep only the last 1000 messages to prevent memory growth.
        if self.messages.len() > 1000 {
            self.messages.drain(0..100);
        }
    }

    /// Scroll up by `n` lines, clamped to the number of messages.
    pub fn scroll_up(&mut self, n: usize) {
        self.scroll_offset = (self.scroll_offset + n).min(self.messages.len().saturating_sub(1));
    }

    /// Scroll down by `n` lines, clamped to 0 (bottom).
    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }
}
