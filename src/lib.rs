//! deckhand: present markdown slide decks as a TUI.
//!
//! The portable core (deck parsing, markdown rendering, theming) compiles
//! for both native and wasm32; the interactive presenter, PTYs, notes
//! socket, and CLI are native-only, and `web` is the wasm-bindgen surface
//! used by the website's in-browser presenter.

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
