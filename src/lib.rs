//! Present markdown slide decks as a TUI.
//!
//! deckhand turns a plain markdown file (or a JSON manifest) into a
//! full-screen terminal presentation with **live embedded terminals**, a
//! **two-dimensional slide grid**, an **overview with quick-jump codes**,
//! and **presenter notes** streamed to a second terminal. The same core
//! compiles to WebAssembly and powers an in-browser presenter at
//! <https://deckhand.sh>.
//!
//! Most people want the binary:
//!
//! ```text
//! deckhand talk.md            # present
//! deckhand notes              # presenter notes, in another terminal
//! deckhand compile deck.json  # flatten a manifest to one file
//! ```
//!
//! # The deck format, in one slide
//!
//! ````text
//! ---
//! title: my talk
//! theme:
//!   border_type: rounded
//! ---
//!
//! # hello                     ← column 1
//!
//! ---
//!
//! # demo                      ← column 2 (`---` = next column)
//!
//! ```terminal rows=10
//! python3 -q
//! ```
//!
//! ???
//! presenter notes live after the ??? line
//!
//! --
//!
//! ## the weeds                ← deeper in column 2 (`--` = down)
//! ````
//!
//! Terminals are real PTYs when presenting natively; on the web they
//! replay captures baked in by `deckhand compile --snapshots`.
//!
//! # Architecture
//!
//! The crate is split so every presentation context shares one brain:
//!
//! - [`presenter::Presenter`] — the platform-independent core: 2-D
//!   navigation, layout, theming, the overview, focus/selection, and all
//!   drawing into a ratatui `Buffer`. It receives neutral
//!   [`presenter::KeyPress`]/[`presenter::Mouse`] events.
//! - [`presenter::TerminalProvider`] — the extension point supplying the
//!   live innards of terminal blocks. The native front end provides real
//!   PTYs ([`present::PtyProvider`]); the wasm front end replays
//!   snapshots; your embedding can provide ssh sessions, containers, or
//!   recordings.
//! - Front ends: [`present`] (native TUI, crossterm) and `web`
//!   (wasm-bindgen + xterm.js; compiled only for wasm32), each a thin
//!   translation layer.
//!
//! # Module map
//!
//! | module | role |
//! |--------|------|
//! | [`deck`] | deck model + single-file markdown parsing |
//! | [`config`] | JSON manifest loading |
//! | [`markdown`] | markdown → styled ratatui text |
//! | [`theme`] | theming: layout, borders, colors, precedence |
//! | [`presenter`] | shared presentation core + provider trait |
//! | [`hints`] | overview quick-jump codes |
//! | [`compile`] | flatten any deck to single-file markdown |
//! | [`proto`] | presenter → notes-client wire format |
//! | native-only | [`present`], [`term`], [`server`], [`notes`], [`source`], [`snapshot`], [`cli`] |
//! | wasm-only | `web` |
//!
//! # Library quickstart
//!
//! Parse a deck and walk it with the shared presenter (a tiny provider
//! stands in for terminals):
//!
//! ```
//! use deckhand::deck;
//! use deckhand::presenter::{
//!     Key, KeyPress, Presenter, TermState, TerminalProvider,
//! };
//! use ratatui::buffer::Buffer;
//! use ratatui::layout::Rect;
//!
//! struct NoTerms;
//! impl TerminalProvider for NoTerms {
//!     fn prepare(&mut self, _: &deck::TermBlock, _: u16, _: u16) {}
//!     fn state(&self, _: &deck::TermBlock) -> TermState {
//!         TermState::Unavailable
//!     }
//!     fn draw(&mut self, _: &deck::TermBlock, _: Rect, _: &mut Buffer) {}
//!     fn input(&mut self, _: usize, _: &[u8]) {}
//!     fn scroll(&mut self, _: usize, _: isize) {}
//!     fn restart(&mut self, _: &[usize]) {}
//!     fn interactive(&self) -> bool {
//!         false
//!     }
//! }
//!
//! let deck = deck::parse("# hello\n---\n# world\n--\n## deeper\n", "demo")?;
//! let mut presenter = Presenter::new(deck, vec![], NoTerms)?;
//!
//! presenter.on_key(KeyPress::plain(Key::Char(' '))); // next slide
//! assert_eq!(presenter.position(), (1, 0));
//!
//! let area = Rect::new(0, 0, 80, 24);
//! let mut frame = Buffer::empty(area);
//! presenter.draw(area, &mut frame); // render anywhere a Buffer can go
//! # Ok::<(), anyhow::Error>(())
//! ```

#![warn(missing_docs)]

pub mod compile;
pub mod config;
pub mod deck;
pub mod hints;
pub mod markdown;
pub mod presenter;
pub mod proto;
pub mod theme;

#[cfg(not(target_arch = "wasm32"))]
pub mod cli;
#[cfg(not(target_arch = "wasm32"))]
pub mod notes;
#[cfg(not(target_arch = "wasm32"))]
pub mod present;
#[cfg(not(target_arch = "wasm32"))]
pub mod server;
#[cfg(not(target_arch = "wasm32"))]
pub mod snapshot;
#[cfg(not(target_arch = "wasm32"))]
pub mod source;
#[cfg(not(target_arch = "wasm32"))]
pub mod term;

#[cfg(target_arch = "wasm32")]
pub mod web;
