//! Shared plumbing for `deckhand compile`'s output formats.
//!
//! Every format walks the same pipeline: load the deck, settle the
//! snapshot-consent question ([`load`]), and — for the page-based
//! exporters (PDF, HTML) — render each slide through the shared
//! [`Presenter`](crate::presenter::Presenter) into an offscreen cell
//! grid ([`render_pages`]) and write the result ([`write`]). This
//! module is that pipeline, once; `compile`, `pdf`, and `html` supply
//! only their format-specific emission.

#[cfg(any(feature = "pdf", feature = "html"))]
use std::path::Path;

use anyhow::{Context, Result};

use crate::{snapshot, source};

/// Default slide-grid width for exported documents, in terminal
/// columns; shared between the exporters' `Options` and the CLI.
pub const DEFAULT_COLS: u16 = 100;
/// Default slide-grid height for exported documents, in terminal rows.
pub const DEFAULT_ROWS: u16 = 30;
/// Default PDF font size, in points.
pub const DEFAULT_FONT_SIZE: f32 = 10.0;

/// Load `input` (a file, URL, or gist) and settle the snapshot
/// question for decks with terminal blocks that have no baked capture:
/// `Some(true)` captures now (running the deck's commands in PTYs),
/// `Some(false)` goes without, and `None` errors, naming the commands
/// — capturing executes them on this machine, and skipping silently
/// would lose their output. Decks whose terminals are all snapshotted
/// (or that have none) need no choice.
pub fn load(
    input: &str,
    snapshots: Option<bool>,
    snapshot_opts: &snapshot::Options,
) -> Result<source::Loaded> {
    let mut loaded = source::load(input)?;
    let missing = loaded.deck.unsnapshotted_commands();
    if !missing.is_empty() {
        match snapshots {
            Some(true) => {
                snapshot::capture(&mut loaded.deck, &loaded.base_dir, snapshot_opts)
                    .context("capturing terminal snapshots")?;
            }
            Some(false) => {}
            None => anyhow::bail!(
                "this deck has {} terminal block(s) without baked snapshots: {}\n\
                 capturing snapshots runs those commands in PTYs on this machine.\n\
                 pass --snapshots to capture their output (only for decks you trust),\n\
                 or --no-snapshots to compile without captures",
                missing.len(),
                missing.join(", "),
            ),
        }
    }
    Ok(loaded)
}

/// Build the offscreen presenter the page-based exporters render
/// through: images resolved, user and deck themes merged, terminal
/// blocks replaying their baked snapshots.
#[cfg(any(feature = "pdf", feature = "html"))]
pub fn presenter(
    loaded: source::Loaded,
) -> Result<crate::presenter::Presenter<crate::replay::SnapshotProvider>> {
    use crate::theme;

    let source::Loaded {
        mut deck,
        theme: deck_theme,
        base_dir,
    } = loaded;
    deck.resolve_images(&base_dir);
    let cfgs: Vec<theme::ThemeConfig> = [theme::user_config()?, deck_theme]
        .into_iter()
        .flatten()
        .collect();
    crate::presenter::Presenter::new(deck, cfgs, crate::replay::SnapshotProvider::default())
}

/// Every slide of a deck rendered into an offscreen cell grid.
#[cfg(any(feature = "pdf", feature = "html"))]
pub struct Pages {
    /// Grid width actually used (requests are clamped to at least 20).
    pub cols: u16,
    /// Grid height actually used (clamped to at least 4). Buffers are
    /// [`crate::presenter::STATUS_ROWS`] taller — the status bar the
    /// presenter always draws, which emitters don't typeset.
    pub rows: u16,
    /// The deck's slides in presentation order, as `((col, row),
    /// rendered grid)`.
    pub slides: Vec<((usize, usize), ratatui::buffer::Buffer)>,
}

/// Render every slide through the presenter into a `cols` × `rows`
/// cell grid — the exact layout, theming, and chrome the TUI shows.
#[cfg(any(feature = "pdf", feature = "html"))]
pub fn render_pages<P: crate::presenter::TerminalProvider>(
    presenter: &mut crate::presenter::Presenter<P>,
    cols: u16,
    rows: u16,
) -> Pages {
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    let cols = cols.max(20);
    let rows = rows.max(4);
    let area = Rect::new(0, 0, cols, rows + crate::presenter::STATUS_ROWS);

    let order = presenter.deck().flat();
    let mut slides = Vec::with_capacity(order.len());
    for &(c, r) in &order {
        presenter.goto(c, r);
        let mut buf = Buffer::empty(area);
        presenter.draw(area, &mut buf);
        slides.push(((c, r), buf));
    }
    Pages { cols, rows, slides }
}

/// Write an exported document to `output` (or to `input`'s file name
/// with `ext`, in the current directory) and report it on stderr.
#[cfg(any(feature = "pdf", feature = "html"))]
pub fn write(
    bytes: &[u8],
    output: Option<&Path>,
    input: &str,
    ext: &str,
    detail: &str,
) -> Result<()> {
    let path = match output {
        Some(p) => p.to_path_buf(),
        None => source::output_name(input, ext),
    };
    std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    eprintln!("wrote {} ({detail})", path.display());
    Ok(())
}

#[cfg(all(test, any(feature = "pdf", feature = "html")))]
mod tests {
    use super::*;
    use crate::presenter::{Presenter, STATUS_ROWS};
    use crate::replay::SnapshotProvider;

    fn presenter_for(src: &str) -> Presenter<SnapshotProvider> {
        let deck = crate::deck::parse(src, "t").unwrap();
        Presenter::new(deck, vec![], SnapshotProvider::default()).unwrap()
    }

    #[test]
    fn pages_render_content_above_a_status_row() {
        let mut p = presenter_for("# alpha\n---\n# beta\n");
        let pages = render_pages(&mut p, 40, 12);
        assert_eq!(pages.slides.len(), 2);
        assert_eq!(pages.slides[0].0, (0, 0));
        assert_eq!(pages.slides[1].0, (1, 0));

        let (_, buf) = &pages.slides[0];
        assert_eq!(buf.area.height, pages.rows + STATUS_ROWS);
        // The status bar lands in the extra rows, never in the page.
        let row_text = |y: u16| -> String {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect()
        };
        let status: String = (pages.rows..buf.area.height).map(row_text).collect();
        assert!(status.contains("1/2"), "status row missing: {status:?}");
        let page: String = (0..pages.rows).map(row_text).collect();
        assert!(page.contains("alpha"), "slide content missing");
        assert!(!page.contains("1/2"), "status leaked into the page");
    }

    #[test]
    fn degenerate_grids_are_clamped() {
        let mut p = presenter_for("# a\n");
        let pages = render_pages(&mut p, 1, 1);
        assert_eq!((pages.cols, pages.rows), (20, 4));
    }
}
