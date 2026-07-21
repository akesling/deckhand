//! Wire format between the presenter and the notes client:
//! newline-delimited JSON over a unix socket, presenter → client only.

use serde::{Deserialize, Serialize};

/// One update from the presenter: where it is, the current notes, and
/// what's next. Broadcast on every slide change and to newly-connected
/// clients.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesState {
    /// Title of the whole deck.
    pub deck_title: String,
    /// Current column (0-based).
    pub col: usize,
    /// Current depth within the column (0-based).
    pub row: usize,
    /// Number of columns in the deck.
    pub total_cols: usize,
    /// Number of slides in the current column.
    pub col_depth: usize,
    /// 1-based position in depth-first traversal order.
    pub slide_no: usize,
    /// Total slides in traversal order.
    pub total_slides: usize,
    /// Current slide's title.
    pub title: String,
    /// Current slide's presenter notes (markdown; may be empty).
    pub notes: String,
    /// Title of the next slide in traversal order, if any.
    pub next_title: Option<String>,
    /// Unix epoch seconds when the presentation started.
    pub started_at: u64,
}
