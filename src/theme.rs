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

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ColorSpec {
    Index(u8),
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_bg: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_fg: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h1: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h2: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullet: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inline_code: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_bg: Option<ColorSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_fg: Option<ColorSpec>,
}

impl ThemeConfig {
    /// Field-wise overlay: values set in `over` win.
    pub fn merged(self, over: ThemeConfig) -> ThemeConfig {
        ThemeConfig {
            max_width: over.max_width.or(self.max_width),
            max_height: over.max_height.or(self.max_height),
            margin: over.margin.or(self.margin),
            vertical_align: over.vertical_align.or(self.vertical_align),
            border_type: over.border_type.or(self.border_type),
            accent: over.accent.or(self.accent),
            muted: over.muted.or(self.muted),
            term_border: over.term_border.or(self.term_border),
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VAlign {
    Center,
    Top,
}

#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub max_width: u16,
    pub max_height: u16,
    pub margin: u16,
    pub vertical_align: VAlign,
    pub border_type: BorderType,
    pub accent: Color,
    pub muted: Color,
    pub term_border: Color,
    pub status_bg: Color,
    pub status_fg: Color,
    pub h1: Color,
    pub h2: Color,
    pub bullet: Color,
    pub quote: Color,
    pub link: Color,
    pub inline_code: Color,
    pub code_bg: Color,
    pub code_fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Theme {
            max_width: 96,
            max_height: u16::MAX,
            margin: 2,
            vertical_align: VAlign::Center,
            border_type: BorderType::Plain,
            accent: Color::Cyan,
            // Mid-gray from the 256-color palette rather than ANSI
            // DarkGray: schemes like solarized remap ANSI 0-15, turning
            // DarkGray into near-background (invisible borders).
            muted: Color::Indexed(244),
            term_border: Color::Indexed(244),
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
        }
    }
}

impl Theme {
    /// Merge configs (lowest precedence first) over the defaults.
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
        Ok(t)
    }
}

fn user_config_path() -> Option<PathBuf> {
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
    fn bad_values_error() {
        let cfg: ThemeConfig = serde_json::from_str(r#"{ "accent": "chartreuse" }"#).unwrap();
        assert!(Theme::resolve(vec![cfg]).is_err());
        let cfg: ThemeConfig = serde_json::from_str(r#"{ "border_type": "wavy" }"#).unwrap();
        assert!(Theme::resolve(vec![cfg]).is_err());
    }
}
