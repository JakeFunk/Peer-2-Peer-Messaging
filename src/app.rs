// ── UI types ──────────────────────────────────────────────────────────────────

/*
Struct:     -ChatMessage
Purpose:    -Represents a single chat message displayed in the UI.

Fields:
            - u64 id:  Unique identifier for the message. Used for cooperative
              deletion across peers so that all participants can remove the
              same message consistently.
            - String sender:  The display name or identifier of the message sender.
            - String content:  The textual content of the message.
            - bool encrypted:  Indicates whether the message was received in
              encrypted form (true) or plaintext (false).

Details:
            - This struct represents user-visible chat messages only.
            - The `id` field enables distributed deletion by uniquely identifying
              each message across the network.
            - The `encrypted` flag can be used by the UI to visually indicate
              whether the message was encrypted.
*/
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Unique ID used for cooperative deletion across peers.
    pub id: u64,
    pub sender: String,
    pub content: String,
    pub encrypted: bool,
}


/*
Enum:       -UiMessage
Purpose:    -Represents all message types that can be rendered or processed by the UI.

Variants:
            - Chat(ChatMessage):  A standard user chat message.
            - System(String):  A system-generated informational message.
            - Delete(u64):  Instruction to remove a chat message with the given ID.

Details:
            - This enum abstracts different kinds of UI events into a single type.
            - The Delete variant is used to propagate message deletion events
              across peers and instruct the UI to remove the message locally.
            - System messages are informational and not associated with a user.
*/
#[derive(Debug, Clone)]
pub enum UiMessage {
    Chat(ChatMessage),
    System(String),
    /// Instructs the UI to remove the chat message with this ID.
    Delete(u64),
}

// ── Modal editing ─────────────────────────────────────────────────────────────
/*
Enum:       -Mode
Purpose:    -Defines the current input mode of the application.

Variants:
            - Insert:  Typing mode where key presses are appended to the input buffer.
            - Normal:  Command mode where control key combinations trigger actions.

Details:
            - Insert mode allows the user to compose messages normally.
            - Normal mode enables command-style controls:
                - Ctrl+C: Quit the application.
                - Ctrl+D: Delete the most recent message sent by this user.
            - Mode switching allows for modal interaction similar to modal text editors.
*/
#[derive(PartialEq)]
pub enum Mode {
    /// Typing mode – keys go into the input buffer.
    Insert,
    /// Command mode – Ctrl+C quits, Ctrl+D deletes last message everywhere.
    Normal,
}

// ── App state ─────────────────────────────────────────────────────────────────
/*
Struct:     -App
Purpose:    -Maintains the complete runtime state of the chat user interface.

Fields:
            - String input:  The current text input buffer.
            - Vec<UiMessage> messages:  List of all messages displayed in the UI.
            - Mode mode:  Current interaction mode (Insert or Normal).
            - Vec<u64> my_sent_ids:  IDs of messages sent by this user, stored
              oldest-first to support cooperative deletion.
            - usize scroll_offset:  Number of lines scrolled up from the bottom.
              A value of 0 indicates the view is pinned to the newest messages.

Details:
            - This struct acts as the central state container for the UI.
            - It manages message storage, deletion, scrolling, and input mode.
            - Message history is bounded to prevent unbounded memory growth.
*/
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

/*
Function:   -new
Purpose:    -Create and initialize a new App instance with default state.

Parameters:
            - None

Details:
            - Initializes an empty input buffer.
            - Initializes an empty message list.
            - Sets the initial mode to Insert.
            - Initializes an empty list of sent message IDs.
            - Sets scroll_offset to 0 (view pinned to bottom).
            - Returns a fully initialized App instance.
*/
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

    /*
    Function:   -add_message
    Purpose:    -Add a new UI message to the application state and handle deletions.

    Parameters:
                - UiMessage msg:  The message or event to be processed.

    Details:
                - If the message is a Delete variant:
                    - Removes all chat messages matching the specified ID.
                    - Removes the ID from my_sent_ids if present.
                    - Appends a system notification indicating a message was deleted.
                    - Returns immediately after processing.
                - Otherwise:
                    - Appends the message to the message list.
                - Maintains a rolling history limit of 1000 messages.
                - If the message count exceeds 1000, removes the oldest 100 messages.
                - Prevents unbounded memory growth during long sessions.
    */
    pub fn add_message(&mut self, msg: UiMessage) {
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
        if self.messages.len() > 1000 {
            self.messages.drain(0..100);
        }
    }

    /*
    Function:   -scroll_up
    Purpose:    -Scroll the message view upward by a specified number of lines.

    Parameters:
                - usize n:  Number of lines to scroll upward.

    Details:
                - Increases scroll_offset by n.
                - Clamps the value so it does not exceed the number of available messages.
                - Uses saturating_sub to prevent underflow when message list is empty.
                - Ensures scrolling remains within valid bounds.
    */
    pub fn scroll_up(&mut self, n: usize) {
        self.scroll_offset = (self.scroll_offset + n).min(self.messages.len().saturating_sub(1));
    }


    /*
    Function:   -scroll_down
    Purpose:    -Scroll the message view downward toward the newest messages.

    Parameters:
                - usize n:  Number of lines to scroll downward.

    Details:
                - Decreases scroll_offset by n.
                - Uses saturating_sub to prevent underflow.
                - A scroll_offset of 0 indicates the view is pinned to the bottom.
                - Ensures scrolling remains within valid bounds.
    */
    pub fn scroll_down(&mut self, n: usize) {
        self.scroll_offset = self.scroll_offset.saturating_sub(n);
    }
}
