//! Presenter-notes follower: connect from any other terminal with
//! `deckhand notes` and it tracks the presenter live over the unix socket.

use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::markdown;
use crate::proto::NotesState;
use crate::theme::{self, Theme};

enum Msg {
    Connected,
    Disconnected,
    State(NotesState),
}

pub fn run(socket: PathBuf) -> Result<()> {
    let theme = Theme::resolve(theme::user_config()?.into_iter().collect())?;
    let (tx, rx) = mpsc::channel::<Msg>();
    {
        let socket = socket.clone();
        std::thread::spawn(move || {
            loop {
                if let Ok(stream) = UnixStream::connect(&socket) {
                    if tx.send(Msg::Connected).is_err() {
                        return;
                    }
                    let reader = BufReader::new(stream);
                    for line in reader.lines() {
                        let Ok(line) = line else { break };
                        if let Ok(state) = serde_json::from_str::<NotesState>(&line)
                            && tx.send(Msg::State(state)).is_err()
                        {
                            return;
                        }
                    }
                    if tx.send(Msg::Disconnected).is_err() {
                        return;
                    }
                }
                std::thread::sleep(Duration::from_millis(1000));
                // Detect UI exit so this thread doesn't linger forever.
                if tx.send(Msg::Disconnected).is_err() {
                    return;
                }
            }
        });
    }

    let mut state: Option<NotesState> = None;
    let mut connected = false;
    let mut terminal = ratatui::init();
    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|f| draw(f, state.as_ref(), connected, &socket, &theme))?;
            if event::poll(Duration::from_millis(250))?
                && let Event::Key(k) = event::read()?
                && matches!(k.kind, KeyEventKind::Press | KeyEventKind::Repeat)
            {
                match k.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    _ => {}
                }
            }
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    Msg::Connected => connected = true,
                    Msg::Disconnected => connected = false,
                    Msg::State(s) => state = Some(s),
                }
            }
        }
    })();
    ratatui::restore();
    result
}

fn draw(
    f: &mut Frame,
    state: Option<&NotesState>,
    connected: bool,
    socket: &std::path::Path,
    theme: &Theme,
) {
    let area = f.area();
    let Some(state) = state else {
        f.render_widget(
            Paragraph::new(vec![
                Line::default(),
                Line::from(" deckhand notes").style(Style::default().add_modifier(Modifier::BOLD)),
                Line::default(),
                Line::from(format!(
                    " waiting for a presenter on {} …",
                    socket.display()
                ))
                .style(Style::default().fg(theme.muted)),
                Line::default(),
                Line::from(" start one with: deckhand present <deck.md>")
                    .style(Style::default().fg(theme.muted)),
            ]),
            area,
        );
        return;
    };

    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(1),
        Constraint::Length(2),
    ])
    .areas(area);

    // Header: deck title, position, elapsed clock.
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let elapsed = now.saturating_sub(state.started_at);
    let clock = format!(
        " {:01}:{:02}:{:02} ",
        elapsed / 3600,
        (elapsed % 3600) / 60,
        elapsed % 60
    );
    let mut left = format!(
        " {} · slide {}/{} · {}.{}",
        state.deck_title,
        state.slide_no,
        state.total_slides,
        state.col + 1,
        state.row + 1
    );
    if state.col_depth > 1 {
        left.push_str(&format!(" (depth {}/{})", state.row + 1, state.col_depth));
    }
    if !connected {
        left.push_str("  ·  PRESENTER OFFLINE");
    }
    let header_style = if connected {
        Style::default().bg(theme.status_bg).fg(theme.status_fg)
    } else {
        Style::default().bg(Color::Red).fg(Color::White)
    };
    let top = Rect {
        height: 1,
        ..header
    };
    f.render_widget(Paragraph::new(left).style(header_style), top);
    f.render_widget(
        Paragraph::new(
            Line::from(Span::styled(
                clock,
                header_style.add_modifier(Modifier::BOLD),
            ))
            .right_aligned(),
        ),
        top,
    );

    // Body: current slide title + notes.
    let inner_w = body.width.saturating_sub(2);
    let mut lines = vec![
        Line::default(),
        Line::from(Span::styled(
            state.title.clone(),
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )),
        Line::default(),
    ];
    if state.notes.trim().is_empty() {
        lines.push(Line::from(Span::styled(
            "(no notes for this slide)",
            Style::default().fg(theme.muted),
        )));
    } else {
        lines.extend(markdown::render(&state.notes, inner_w.max(10), theme).lines);
    }
    f.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::NONE)),
        Rect {
            x: body.x + 1,
            y: body.y,
            width: inner_w,
            height: body.height,
        },
    );

    // Footer: what's next.
    let next = match &state.next_title {
        Some(t) => format!(" next → {t}"),
        None => " next → (end of deck)".to_string(),
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            next,
            Style::default().fg(theme.muted),
        ))),
        Rect {
            y: footer.y + 1,
            height: 1,
            ..footer
        },
    );
}
