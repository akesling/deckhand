//! Theming: layout, borders, and colors.
//!
//! Sources, lowest to highest precedence, merged field by field:
//!   1. built-in defaults (matches deckhand's out-of-the-box look)
//!   2. `~/.config/deckhand/theme.json` (or `$XDG_CONFIG_HOME/deckhand/theme.json`)
//!   3. the `"theme"` section of a JSON deck manifest
//!
//! Colors are names ("cyan", "light-blue"), hex ("#rrggbb"), or a 0-255
//! palette index (as a JSON number or string).

use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Context, Result, bail};
use ratatui::style::Color;
use ratatui::widgets::BorderType;
use serde::{Deserialize, Serialize};

/// A color as authored in configuration: a 0-255 palette index, or a
/// string holding a name ("cyan", "light-blue") or hex ("#rrggbb").
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ColorSpec {
    /// A 256-color palette index.
    Index(u8),
    /// A color name or `#rrggbb` hex value.
    Name(String),
}

impl ColorSpec {
    fn to_color(&self) -> Result<Color> {
        match self {
            ColorSpec::Index(n) => Ok(Color::Indexed(*n)),
            ColorSpec::Name(raw) => {
                let raw = raw.trim();
                if let Ok(c) = Color::from_str(raw) {
                    return Ok(c);
                }
                // "light blue" / "light_blue" → "lightblue"
                let squashed: String = raw
                    .to_lowercase()
                    .chars()
                    .filter(|c| !matches!(c, ' ' | '-' | '_'))
                    .collect();
                Color::from_str(&squashed).map_err(|_| {
                    anyhow::anyhow!(
                        "unrecognized color {raw:?} (use a name like \"light-blue\", \
                         \"#rrggbb\" hex, or a 0-255 palette index)"
                    )
                })
            }
        }
    }
}

/// All-optional theme as it appears in JSON manifests or markdown
/// frontmatter. Missing fields fall through to the next source in the
/// precedence chain.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeConfig {
    /// Max content width in columns (default 96).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_width: Option<u16>,
    /// Max content height in rows (default unlimited). Caps how tall the
    /// slide content box gets — including fill terminals.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_height: Option<u16>,
    /// Minimum horizontal margin per side, in columns (default 2).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin: Option<u16>,
    /// "center" (default) or "top".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_align: Option<String>,
    /// Anchor a slide-leading heading to the top of the slide; the
    /// body below it still follows `vertical_align`. Default false.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pin_title: Option<bool>,
    /// "plain" (default), "rounded", "double", or "thick".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_type: Option<String>,
    /// Focused borders, overview selection, help border, key hints.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<ColorSpec>,
    /// Rules, hints, separators, unfocused chrome.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub muted: Option<ColorSpec>,
    /// Unfocused terminal borders.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term_border: Option<ColorSpec>,
    /// Borders of terminals showing a baked-in snapshot; supersedes
    /// `term_border` for those (unset = inherit `term_border`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_border: Option<ColorSpec>,
    /// Status bar background.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_bg: Option<ColorSpec>,
    /// Status bar text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_fg: Option<ColorSpec>,
    /// Level-1 headings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h1: Option<ColorSpec>,
    /// Level-2 headings.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h2: Option<ColorSpec>,
    /// List bullets and numbers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullet: Option<ColorSpec>,
    /// Blockquote markers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<ColorSpec>,
    /// Link text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<ColorSpec>,
    /// Inline code spans.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_code: Option<ColorSpec>,
    /// Code block background.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_bg: Option<ColorSpec>,
    /// Code block text.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_fg: Option<ColorSpec>,
    /// QR code modules (default true black). Keep it darker than
    /// `qr_light` and high-contrast, or scanners will give up; light
    /// codes on dark backgrounds fail on many readers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_dark: Option<ColorSpec>,
    /// QR code background, quiet zone included (default true white).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub qr_light: Option<ColorSpec>,
}

impl ThemeConfig {
    /// Field-wise overlay: values set in `over` win.
    pub fn merged(self, over: ThemeConfig) -> ThemeConfig {
        ThemeConfig {
            max_width: over.max_width.or(self.max_width),
            max_height: over.max_height.or(self.max_height),
            margin: over.margin.or(self.margin),
            vertical_align: over.vertical_align.or(self.vertical_align),
            pin_title: over.pin_title.or(self.pin_title),
            border_type: over.border_type.or(self.border_type),
            accent: over.accent.or(self.accent),
            muted: over.muted.or(self.muted),
            term_border: over.term_border.or(self.term_border),
            snapshot_border: over.snapshot_border.or(self.snapshot_border),
            status_bg: over.status_bg.or(self.status_bg),
            status_fg: over.status_fg.or(self.status_fg),
            h1: over.h1.or(self.h1),
            h2: over.h2.or(self.h2),
            bullet: over.bullet.or(self.bullet),
            quote: over.quote.or(self.quote),
            link: over.link.or(self.link),
            inline_code: over.inline_code.or(self.inline_code),
            code_bg: over.code_bg.or(self.code_bg),
            code_fg: over.code_fg.or(self.code_fg),
            qr_dark: over.qr_dark.or(self.qr_dark),
            qr_light: over.qr_light.or(self.qr_light),
        }
    }
}

/// Where slide content sits when shorter than the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VAlign {
    /// Vertically centered (the default).
    Center,
    /// Anchored to the top.
    Top,
}

/// A fully-resolved theme, ready to render with. Produced by
/// [`Theme::resolve`] from layered [`ThemeConfig`]s.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    /// Max content width in columns.
    pub max_width: u16,
    /// Max content height in rows (`u16::MAX` = unlimited).
    pub max_height: u16,
    /// Minimum horizontal margin per side, in columns.
    pub margin: u16,
    /// Vertical placement of slide content.
    pub vertical_align: VAlign,
    /// Slide-leading headings stick to the top; the body follows
    /// `vertical_align`.
    pub pin_title: bool,
    /// Border style for terminals, overview boxes, and overlays.
    pub border_type: BorderType,
    /// Focused borders, overview selection, help chrome, key hints.
    pub accent: Color,
    /// Rules, hints, separators, unfocused chrome.
    pub muted: Color,
    /// Unfocused live-terminal borders.
    pub term_border: Color,
    /// `None` inherits `term_border`; presenters resolve via
    /// [`Theme::snapshot_border`].
    pub snapshot_border: Option<Color>,
    /// Status bar background.
    pub status_bg: Color,
    /// Status bar text.
    pub status_fg: Color,
    /// Level-1 headings.
    pub h1: Color,
    /// Level-2 headings.
    pub h2: Color,
    /// List bullets and numbers.
    pub bullet: Color,
    /// Blockquote markers.
    pub quote: Color,
    /// Link text.
    pub link: Color,
    /// Inline code spans.
    pub inline_code: Color,
    /// Code block background.
    pub code_bg: Color,
    /// Code block text.
    pub code_fg: Color,
    /// QR code modules.
    pub qr_dark: Color,
    /// QR code background, quiet zone included.
    pub qr_light: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            max_width: 96,
            max_height: u16::MAX,
            margin: 2,
            vertical_align: VAlign::Center,
            pin_title: false,
            border_type: BorderType::Plain,
            accent: Color::Cyan,
            // Mid-gray from the 256-color palette rather than ANSI
            // DarkGray: schemes like solarized remap ANSI 0-15, turning
            // DarkGray into near-background (invisible borders).
            muted: Color::Indexed(244),
            term_border: Color::Indexed(244),
            snapshot_border: None,
            status_bg: Color::Indexed(236),
            status_fg: Color::Indexed(250),
            h1: Color::Cyan,
            h2: Color::LightBlue,
            bullet: Color::Cyan,
            quote: Color::Green,
            link: Color::Blue,
            inline_code: Color::Yellow,
            code_bg: Color::Indexed(235),
            code_fg: Color::Indexed(252),
            // True black-on-white from the 256-color cube — scanners
            // want dark-on-light, and ANSI black/white are remappable.
            qr_dark: Color::Indexed(16),
            qr_light: Color::Indexed(231),
        }
    }
}

impl Theme {
    /// Border color for a snapshot-rendered terminal: its own setting,
    /// or the live-terminal default.
    pub fn snapshot_border(&self) -> Color {
        self.snapshot_border.unwrap_or(self.term_border)
    }

    /// Merge configs (lowest precedence first) over the defaults.
    ///
    /// ```
    /// use deckhand::theme::{Theme, ThemeConfig};
    /// use ratatui::style::Color;
    ///
    /// let user: ThemeConfig = serde_json::from_str(r#"{ "accent": "magenta" }"#)?;
    /// let deck: ThemeConfig = serde_json::from_str(r#"{ "max_width": 60 }"#)?;
    /// let theme = Theme::resolve(vec![user, deck])?;
    /// assert_eq!(theme.accent, Color::Magenta); // user survives
    /// assert_eq!(theme.max_width, 60);          // deck wins its field
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn resolve(configs: Vec<ThemeConfig>) -> Result<Theme> {
        let cfg = configs
            .into_iter()
            .fold(ThemeConfig::default(), ThemeConfig::merged);
        let mut t = Theme::default();
        if let Some(w) = cfg.max_width {
            t.max_width = w.max(20);
        }
        if let Some(h) = cfg.max_height {
            t.max_height = h.max(5);
        }
        if let Some(m) = cfg.margin {
            t.margin = m.min(30);
        }
        if let Some(p) = cfg.pin_title {
            t.pin_title = p;
        }
        if let Some(v) = cfg.vertical_align {
            t.vertical_align = match v.as_str() {
                "center" => VAlign::Center,
                "top" => VAlign::Top,
                other => {
                    bail!("theme.vertical_align: expected \"center\" or \"top\", got {other:?}")
                }
            };
        }
        if let Some(b) = cfg.border_type {
            t.border_type = match b.as_str() {
                "plain" => BorderType::Plain,
                "rounded" => BorderType::Rounded,
                "double" => BorderType::Double,
                "thick" => BorderType::Thick,
                other => bail!(
                    "theme.border_type: expected \"plain\", \"rounded\", \"double\", or \"thick\", got {other:?}"
                ),
            };
        }
        macro_rules! color {
            ($field:ident) => {
                if let Some(c) = &cfg.$field {
                    t.$field = c
                        .to_color()
                        .with_context(|| concat!("theme.", stringify!($field)))?;
                }
            };
        }
        color!(accent);
        color!(muted);
        color!(term_border);
        if let Some(c) = &cfg.snapshot_border {
            t.snapshot_border = Some(c.to_color().context("theme.snapshot_border")?);
        }
        color!(status_bg);
        color!(status_fg);
        color!(h1);
        color!(h2);
        color!(bullet);
        color!(quote);
        color!(link);
        color!(inline_code);
        color!(code_bg);
        color!(code_fg);
        color!(qr_dark);
        color!(qr_light);
        Ok(t)
    }
}

/// Where the user-level theme lives:
/// `$XDG_CONFIG_HOME/deckhand/theme.json` or `~/.config/deckhand/theme.json`.
pub fn user_config_path() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("deckhand").join("theme.json"));
    }
    std::env::var("HOME").ok().map(|h| {
        PathBuf::from(h)
            .join(".config")
            .join("deckhand")
            .join("theme.json")
    })
}

/// The user-level theme file, if one exists. A malformed file is an error
/// (silently ignoring it would make debugging a typo miserable).
pub fn user_config() -> Result<Option<ThemeConfig>> {
    let Some(path) = user_config_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let src = std::fs::read_to_string(&path)
        .with_context(|| format!("reading theme {}", path.display()))?;
    let cfg =
        serde_json::from_str(&src).with_context(|| format!("parsing theme {}", path.display()))?;
    Ok(Some(cfg))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_empty() {
        let t = Theme::resolve(vec![]).unwrap();
        assert_eq!(t.max_width, 96);
        assert_eq!(t.accent, Color::Cyan);
    }

    #[test]
    fn merge_precedence() {
        let user: ThemeConfig =
            serde_json::from_str(r#"{ "accent": "magenta", "max_width": 80 }"#).unwrap();
        let deck: ThemeConfig =
            serde_json::from_str(r#"{ "max_width": 60, "max_height": 25 }"#).unwrap();
        let t = Theme::resolve(vec![user, deck]).unwrap();
        assert_eq!(t.max_width, 60); // deck wins
        assert_eq!(t.max_height, 25);
        assert_eq!(t.accent, Color::Magenta); // user survives
    }

    #[test]
    fn color_forms() {
        let cfg: ThemeConfig = serde_json::from_str(
            r##"{ "h1": "#ff8800", "code_bg": 17, "bullet": "light-green" }"##,
        )
        .unwrap();
        let t = Theme::resolve(vec![cfg]).unwrap();
        assert_eq!(t.h1, Color::Rgb(255, 136, 0));
        assert_eq!(t.code_bg, Color::Indexed(17));
        assert_eq!(t.bullet, Color::LightGreen);
    }

    #[test]
    fn snapshot_border_supersedes_term_border() {
        // Unset: inherits the live-terminal border.
        let t = Theme::resolve(vec![]).unwrap();
        assert_eq!(t.snapshot_border(), t.term_border);

        let cfg: ThemeConfig =
            serde_json::from_str(r#"{ "term_border": "blue", "snapshot_border": "yellow" }"#)
                .unwrap();
        let t = Theme::resolve(vec![cfg]).unwrap();
        assert_eq!(t.term_border, Color::Blue);
        assert_eq!(t.snapshot_border(), Color::Yellow);
    }

    #[test]
    fn pin_title_resolves_and_merges() {
        let t = Theme::resolve(vec![]).unwrap();
        assert!(!t.pin_title);
        let user: ThemeConfig = serde_json::from_str(r#"{ "pin_title": true }"#).unwrap();
        let deck: ThemeConfig = serde_json::from_str(r#"{ "accent": "red" }"#).unwrap();
        let t = Theme::resolve(vec![user, deck]).unwrap();
        assert!(t.pin_title); // survives an overlay that doesn't set it
    }

    #[test]
    fn qr_colors_default_to_true_black_on_white() {
        let t = Theme::resolve(vec![]).unwrap();
        assert_eq!(t.qr_dark, Color::Indexed(16));
        assert_eq!(t.qr_light, Color::Indexed(231));

        let cfg: ThemeConfig =
            serde_json::from_str(r##"{ "qr_dark": "#102040", "qr_light": "#fdf6e3" }"##).unwrap();
        let t = Theme::resolve(vec![cfg]).unwrap();
        assert_eq!(t.qr_dark, Color::Rgb(0x10, 0x20, 0x40));
        assert_eq!(t.qr_light, Color::Rgb(0xfd, 0xf6, 0xe3));
    }

    #[test]
    fn bad_values_error() {
        let cfg: ThemeConfig = serde_json::from_str(r#"{ "accent": "chartreuse" }"#).unwrap();
        assert!(Theme::resolve(vec![cfg]).is_err());
        let cfg: ThemeConfig = serde_json::from_str(r#"{ "border_type": "wavy" }"#).unwrap();
        assert!(Theme::resolve(vec![cfg]).is_err());
    }
}
