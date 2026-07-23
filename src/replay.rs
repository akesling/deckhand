//! A [`TerminalProvider`] for contexts that can't run terminals.
//!
//! Blocks with a baked-in snapshot (from `deckhand compile --snapshots`)
//! replay it through the same vt100 → tui-term path the native presenter
//! uses; the rest get an honest placeholder. Neither can be focused.
//! Shared by the wasm presenter (browsers have no PTYs) and `deckhand
//! pdf` (a page has no PTYs either).

use std::collections::HashMap;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use crate::deck::TermBlock;
use crate::presenter::{TermState, TerminalProvider};

/// Replays [`crate::deck::TermSnapshot`]s; see the [module docs](self).
#[derive(Default)]
pub struct SnapshotProvider {
    parsers: HashMap<usize, vt100::Parser>,
}

impl TerminalProvider for SnapshotProvider {
    fn prepare(&mut self, block: &TermBlock, _cols: u16, _rows: u16) {
        let Some(snap) = &block.snapshot else { return };
        self.parsers.entry(block.id).or_insert_with(|| {
            // Enough scrollback to wheel through a recording's output.
            let mut parser = vt100::Parser::new(snap.rows, snap.cols, 1000);
            parser.process(&snap.data);
            parser
        });
    }

    fn state(&self, block: &TermBlock) -> TermState {
        if block.snapshot.is_some() {
            TermState::Snapshot
        } else {
            TermState::Unavailable
        }
    }

    fn draw(&mut self, block: &TermBlock, inner: Rect, buf: &mut Buffer) {
        if let Some(parser) = self.parsers.get(&block.id) {
            tui_term::widget::PseudoTerminal::new(parser.screen()).render(inner, buf);
            return;
        }
        let muted = Style::default().fg(Color::Indexed(244));
        let msg = vec![
            Line::default(),
            Line::from(Span::styled("  ▶ interactive terminal", muted)),
            Line::from(Span::styled(
                "    spawns a real PTY when you present in your own terminal",
                muted.add_modifier(Modifier::ITALIC),
            )),
        ];
        Paragraph::new(msg).render(inner, buf);
    }

    fn input(&mut self, _: usize, _: &[u8]) {}

    fn scroll(&mut self, id: usize, delta: isize) {
        if let Some(p) = self.parsers.get_mut(&id) {
            // Alternate-screen snapshots (full-screen TUIs) are a
            // still frame — there's no history to move through.
            if p.screen().alternate_screen() {
                return;
            }
            let cur = p.screen().scrollback() as isize;
            p.set_scrollback((cur + delta).max(0) as usize);
        }
    }
    fn restart(&mut self, _: &[usize]) {}

    fn reset(&mut self) {
        self.parsers.clear();
    }

    fn interactive(&self) -> bool {
        false
    }
}
