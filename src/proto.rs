//! Wire format between the presenter and the notes client:
//! newline-delimited JSON over a unix socket, presenter → client only.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotesState {
    pub deck_title: String,
    pub col: usize,
    pub row: usize,
    pub total_cols: usize,
    pub col_depth: usize,
    /// 1-based position in depth-first traversal order.
    pub slide_no: usize,
    pub total_slides: usize,
    pub title: String,
    pub notes: String,
    pub next_title: Option<String>,
    /// Unix epoch seconds when the presentation started.
    pub started_at: u64,
}
