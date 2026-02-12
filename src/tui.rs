use std::io;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event as CEvent, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Terminal,
};
use tokio::sync::mpsc;

use crate::app::{App, ChatMessage, Mode, UiMessage};

// ── TUI ───────────────────────────────────────────────────────────────────────

pub async fn run_tui(
    mut ui_rx: mpsc::Receiver<UiMessage>,
    input_tx: mpsc::Sender<(String, u64)>,
    delete_tx: mpsc::Sender<u64>,
) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    loop {
        // Drain incoming messages from gossip / system.
        while let Ok(msg) = ui_rx.try_recv() {
            app.add_message(msg);
        }

        // ── Draw ─────────────────────────────────────────────────────────────
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Header / mode indicator
                    Constraint::Min(0),    // Messages
                    Constraint::Length(3), // Input
                    Constraint::Length(5), // Controls
                ])
                .split(f.area());

            // Header shows current mode prominently.
            let (mode_label, mode_hint) = match app.mode {
                Mode::Insert => (
                    Span::styled(
                        " INSERT ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "  ESC → normal mode",
                        Style::default().fg(Color::DarkGray),
                    ),
                ),
                Mode::Normal => (
                    Span::styled(
                        " NORMAL ",
                        Style::default()
                            .fg(Color::Black)
                            .bg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "  i → insert  |  Ctrl+D → delete last msg  |  Ctrl+C → quit",
                        Style::default().fg(Color::DarkGray),
                    ),
                ),
            };

            let header = Paragraph::new(vec![Line::from(vec![
                Span::styled(
                    "Encrypted Chat  ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                mode_label,
                mode_hint,
            ])])
            .block(Block::default().borders(Borders::ALL));
            f.render_widget(header, chunks[0]);

            // Messages list — scroll_offset=0 means pinned to bottom.
            let messages: Vec<ListItem> = app
                .messages
                .iter()
                .map(|m| match m {
                    UiMessage::Chat(chat) => ListItem::new(Line::from(vec![
                        Span::styled(
                            &chat.sender,
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(": "),
                        Span::styled(&chat.content, Style::default().fg(Color::White)),
                    ])),
                    UiMessage::System(text) => ListItem::new(Line::from(Span::styled(
                        format!("• {}", text),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::ITALIC),
                    ))),
                    UiMessage::Delete(_) => ListItem::new(Line::from("")),
                })
                .collect();

            let total = messages.len();
            let mut list_state = ListState::default();
            if total > 0 {
                // selected index drives what ratatui keeps in view.
                // offset=0 → select the last item (bottom); offset=N → select N from the end.
                let selected = total.saturating_sub(1 + app.scroll_offset);
                list_state.select(Some(selected));
            }

            let messages_widget = List::new(messages)
                .block(Block::default().borders(Borders::ALL).title(
                    if app.scroll_offset > 0 { "Messages  ↑ scrolled" } else { "Messages" }
                ))
                .highlight_style(Style::default()); // no highlight decoration
            f.render_stateful_widget(messages_widget, chunks[1], &mut list_state);

            // Input box – dim it in Normal mode to signal it's inactive.
            let input_style = match app.mode {
                Mode::Insert => Style::default().fg(Color::White),
                Mode::Normal => Style::default().fg(Color::DarkGray),
            };
            let input_title = match app.mode {
                Mode::Insert => "Input",
                Mode::Normal => "Input (press i to type)",
            };
            let input = Paragraph::new(app.input.as_str())
                .style(input_style)
                .block(Block::default().borders(Borders::ALL).title(input_title));
            f.render_widget(input, chunks[2]);

            // Controls help panel.
            let controls_text = match app.mode {
                Mode::Insert => vec![
                    Line::from(vec![
                        Span::styled("Enter", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                        Span::styled("  send message    ", Style::default().fg(Color::Gray)),
                        Span::styled("Backspace", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                        Span::styled("  delete char    ", Style::default().fg(Color::Gray)),
                        Span::styled("ESC", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                        Span::styled("  normal mode", Style::default().fg(Color::Gray)),
                    ]),
                ],
                Mode::Normal => vec![
                    Line::from(vec![
                        Span::styled("i", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled("  insert mode    ", Style::default().fg(Color::Gray)),
                        Span::styled("↑↓ / PgUp PgDn", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled("  scroll    ", Style::default().fg(Color::Gray)),
                        Span::styled("Ctrl+D", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled("  delete last msg    ", Style::default().fg(Color::Gray)),
                        Span::styled("Ctrl+C", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                        Span::styled("  quit", Style::default().fg(Color::Gray)),
                    ]),
                ],
            };
            let controls = Paragraph::new(controls_text)
                .block(Block::default().borders(Borders::ALL).title("Controls"));
            f.render_widget(controls, chunks[3]);
        })?;

        // ── Input handling ────────────────────────────────────────────────────
        if event::poll(std::time::Duration::from_millis(100))? {
            if let CEvent::Key(key) = event::read()? {
                match app.mode {
                    // ── INSERT mode ──────────────────────────────────────────
                    Mode::Insert => match key.code {
                        KeyCode::Esc => {
                            app.mode = Mode::Normal;
                        }
                        KeyCode::Char(c) => {
                            app.input.push(c);
                        }
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        KeyCode::Enter => {
                            if !app.input.is_empty() {
                                let text = app.input.clone();
                                let id: u64 = rand::random();

                                // Show immediately in our own UI.
                                app.add_message(UiMessage::Chat(ChatMessage {
                                    id,
                                    sender: "You".to_string(),
                                    content: text.clone(),
                                    encrypted: true,
                                }));
                                // Remember the ID so we can delete it later.
                                app.my_sent_ids.push(id);

                                let _ = input_tx.send((text, id)).await;
                                app.input.clear();
                            }
                        }
                        _ => {}
                    },

                    // ── NORMAL mode ──────────────────────────────────────────
                    Mode::Normal => match key.code {
                        // Return to typing.
                        KeyCode::Char('i') => {
                            app.mode = Mode::Insert;
                        }

                        // Scroll up/down.
                        KeyCode::Up => { app.scroll_up(1); }
                        KeyCode::Down => { app.scroll_down(1); }
                        KeyCode::PageUp => { app.scroll_up(10); }
                        KeyCode::PageDown => { app.scroll_down(10); }

                        // Quit.
                        KeyCode::Char('c')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            break;
                        }

                        // Delete our most recent message on all peers.
                        KeyCode::Char('d')
                            if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                        {
                            if let Some(id) = app.my_sent_ids.last().copied() {
                                // Remove locally first for instant feedback.
                                app.add_message(UiMessage::Delete(id));
                                // Broadcast the deletion to all peers.
                                let _ = delete_tx.send(id).await;
                            } else {
                                app.add_message(UiMessage::System(
                                    "No messages to delete.".to_string(),
                                ));
                            }
                        }

                        _ => {}
                    },
                }
            }
        }
    }

    // Restore terminal.
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}
