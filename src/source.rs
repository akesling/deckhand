//! Deck loading from local paths, plain URLs, and GitHub gists.
//!
//! `deckhand https://example.com/talk.md` fetches and presents directly.
//! A gist page URL (`https://gist.github.com/user/abc123`) is resolved via
//! the GitHub API to the gist's files; the entry deck is picked by
//! priority: `deck.json`, then `deck.md`, then the only file, then the
//! first `.md` alphabetically. JSON manifests work remotely too — their
//! relative paths fetch against the manifest's URL (or, in a gist, against
//! the gist's other files).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::deck::{self, Deck};
use crate::theme::ThemeConfig;

/// A deck resolved from any source, plus its context.
pub struct Loaded {
    /// The parsed deck.
    pub deck: Deck,
    /// The deck's own theme config (manifest `theme` or frontmatter).
    pub theme: Option<ThemeConfig>,
    /// Working directory for embedded terminals (the deck's directory for
    /// local decks; the current directory for remote ones).
    pub base_dir: PathBuf,
}

/// Load a deck from a local path, a plain URL, or a GitHub gist URL.
pub fn load(input: &str) -> Result<Loaded> {
    if input.starts_with("http://") || input.starts_with("https://") {
        load_remote(input)
    } else {
        let path = Path::new(input);
        let (mut deck, theme) = deck::load(path)?;
        let base_dir = path
            .canonicalize()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| PathBuf::from("."));
        deck.resolve_images(&base_dir);
        Ok(Loaded {
            deck,
            theme,
            base_dir,
        })
    }
}

/// Files whose changes should trigger a live reload for `input`: the
/// deck file itself, everything a manifest references, referenced
/// images, and the user theme file. Empty for remote decks (nothing
/// local to watch).
pub fn watch_paths(input: &str) -> Vec<PathBuf> {
    if input.starts_with("http://") || input.starts_with("https://") {
        return Vec::new();
    }
    let entry = PathBuf::from(input);
    let mut out = vec![entry.clone()];
    let base = entry.parent().unwrap_or(Path::new(".")).to_path_buf();
    if entry.extension().and_then(|e| e.to_str()) == Some("json")
        && let Ok(src) = std::fs::read_to_string(&entry)
    {
        for rel in crate::config::referenced_files(&src) {
            out.push(base.join(rel));
        }
    }
    // Images redraw on reload too; a deck that doesn't parse right now
    // just watches its files.
    if let Ok((deck, _)) = deck::load(&entry) {
        for img in deck.image_paths() {
            out.push(base.join(img));
        }
    }
    if let Some(theme_path) = crate::theme::user_config_path() {
        out.push(theme_path);
    }
    out
}

enum Remote {
    /// Relative paths join onto this URL prefix.
    Http { base: String },
    /// Gist filename → raw content URL.
    Gist { files: BTreeMap<String, String> },
}

impl Remote {
    fn read(&self, rel: &str) -> Result<String> {
        match self {
            Remote::Http { base } => http_get(&format!("{base}/{rel}")),
            Remote::Gist { files } => {
                let url = files.get(rel).with_context(|| {
                    format!(
                        "gist has no file named {rel:?} (it has: {})",
                        files.keys().cloned().collect::<Vec<_>>().join(", ")
                    )
                })?;
                http_get(url)
            }
        }
    }
}

fn load_remote(url: &str) -> Result<Loaded> {
    let (entry, remote) = resolve(url)?;
    let src = remote.read(&entry)?;
    let (deck, theme) = if entry.ends_with(".json") {
        let reader = |p: &Path| remote.read(&p.to_string_lossy());
        crate::config::parse_manifest(&src, url, &reader)?
    } else {
        let fallback = entry.trim_end_matches(".md").trim_end_matches(".markdown");
        let (deck, theme) = deck::parse_full(&src, fallback)?;
        if deck.columns.is_empty() {
            bail!("{url}: deck contains no slides");
        }
        (deck, theme)
    };
    Ok(Loaded {
        deck,
        theme,
        base_dir: PathBuf::from("."),
    })
}

/// Figure out what to fetch: gist page URLs go through the GitHub API,
/// anything else splits into (base, filename).
fn resolve(url: &str) -> Result<(String, Remote)> {
    if let Some(id) = gist_id(url) {
        let body = http_get(&format!("https://api.github.com/gists/{id}"))?;
        let json: serde_json::Value = serde_json::from_str(&body)
            .with_context(|| format!("parsing GitHub API response for gist {id}"))?;
        let files_obj = json
            .get("files")
            .and_then(|f| f.as_object())
            .with_context(|| format!("gist {id}: unexpected API response (no files)"))?;
        let mut files = BTreeMap::new();
        for (name, meta) in files_obj {
            if let Some(raw) = meta.get("raw_url").and_then(|u| u.as_str()) {
                files.insert(name.clone(), raw.to_string());
            }
        }
        let entry = pick_entry(&files.keys().cloned().collect::<Vec<_>>())
            .with_context(|| format!("gist {id}"))?;
        Ok((entry, Remote::Gist { files }))
    } else {
        let clean = url.split(['#', '?']).next().unwrap_or(url);
        let (base, name) = clean
            .rsplit_once('/')
            .with_context(|| format!("{url}: not a valid URL"))?;
        if name.is_empty() || !base.contains("//") || base.ends_with('/') {
            bail!("{url}: URL must point at a deck file, not a directory");
        }
        Ok((
            name.to_string(),
            Remote::Http {
                base: base.to_string(),
            },
        ))
    }
}

/// Extract the gist id from a gist *page* URL. Raw
/// `gist.githubusercontent.com` URLs are plain files and don't match.
fn gist_id(url: &str) -> Option<String> {
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let rest = rest.strip_prefix("gist.github.com/")?;
    let path = rest.split(['#', '?']).next().unwrap_or(rest);
    let seg = path.trim_end_matches('/').rsplit('/').next()?;
    let seg = seg.trim_end_matches(".git");
    (!seg.is_empty() && seg.chars().all(|c| c.is_ascii_hexdigit())).then(|| seg.to_string())
}

/// Which gist file is the deck?
fn pick_entry(names: &[String]) -> Result<String> {
    for preferred in ["deck.json", "deck.md"] {
        if names.iter().any(|n| n == preferred) {
            return Ok(preferred.to_string());
        }
    }
    if let [only] = names {
        return Ok(only.clone());
    }
    if let Some(md) = names.iter().find(|n| n.ends_with(".md")) {
        return Ok(md.clone());
    }
    bail!(
        "can't tell which file is the deck (looked for deck.json, deck.md, or a .md file) among: {}",
        names.join(", ")
    )
}

/// Blocking fetch built on reqwest + a lazily-created tokio runtime —
/// the TUI itself is synchronous, so requests are `block_on`'d here.
fn http_get(url: &str) -> Result<String> {
    use std::sync::OnceLock;
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

    let runtime = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("building tokio runtime")
    });
    let client = CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent("deckhand")
            .build()
            .expect("building HTTP client")
    });
    runtime.block_on(async {
        let response = client
            .get(url)
            .send()
            .await
            .with_context(|| format!("fetching {url}"))?
            .error_for_status()
            .with_context(|| format!("fetching {url}"))?;
        response
            .text()
            .await
            .with_context(|| format!("reading response body from {url}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn gist_ids() {
        assert_eq!(
            gist_id("https://gist.github.com/alex/abc123def").as_deref(),
            Some("abc123def")
        );
        assert_eq!(
            gist_id("https://gist.github.com/abc123def").as_deref(),
            Some("abc123def")
        );
        assert_eq!(
            gist_id("https://gist.github.com/alex/abc123def#file-deck-md").as_deref(),
            Some("abc123def")
        );
        assert_eq!(
            gist_id("https://gist.github.com/alex/abc123def.git").as_deref(),
            Some("abc123def")
        );
        assert_eq!(gist_id("https://example.com/talk.md"), None);
        // raw gist content URLs are plain files, not gist pages
        assert_eq!(
            gist_id("https://gist.githubusercontent.com/alex/abc/raw/deck.md"),
            None
        );
    }

    #[test]
    fn watch_paths_include_referenced_images() {
        let dir = std::env::temp_dir().join(format!("deckhand-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let deck = dir.join("deck.md");
        std::fs::write(&deck, "# a\n![pic](images/arch.png)\n").unwrap();
        let paths = watch_paths(deck.to_str().unwrap());
        assert!(paths.contains(&deck));
        assert!(
            paths.contains(&dir.join("images/arch.png")),
            "paths: {paths:?}"
        );
    }

    #[test]
    fn entry_priority() {
        assert_eq!(
            pick_entry(&s(&["a.md", "deck.json", "deck.md"])).unwrap(),
            "deck.json"
        );
        assert_eq!(pick_entry(&s(&["a.md", "deck.md"])).unwrap(), "deck.md");
        assert_eq!(pick_entry(&s(&["notes.txt"])).unwrap(), "notes.txt");
        assert_eq!(pick_entry(&s(&["b.md", "notes.txt"])).unwrap(), "b.md");
        assert!(pick_entry(&s(&["a.txt", "b.txt"])).is_err());
    }

    #[test]
    fn url_split() {
        let (entry, remote) = resolve_offline("https://example.com/talks/deck.md");
        assert_eq!(entry, "deck.md");
        match remote {
            Remote::Http { base } => assert_eq!(base, "https://example.com/talks"),
            _ => panic!(),
        }
    }

    // resolve() without the network path (non-gist URLs never fetch).
    fn resolve_offline(url: &str) -> (String, Remote) {
        resolve(url).unwrap()
    }

    #[test]
    fn directory_urls_rejected() {
        assert!(resolve("https://example.com/talks/").is_err());
    }
}
