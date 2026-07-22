//! Lightweight syntax highlighting for fenced code blocks.
//!
//! A deliberately small, dependency-free tokenizer: line comments,
//! block comments, strings (including multi-line raw/triple-quoted
//! forms), numbers, keywords, and named literals, for the languages a
//! technical talk usually shows. It is not a grammar — it's stage
//! makeup. Unknown languages render plain, exactly as before.
//!
//! Colors come from the theme: `code_keyword`, `code_string`,
//! `code_comment`, `code_literal`, `code_function`, and `code_type`
//! (over the usual `code_bg`/`code_fg`), defaulting to a One
//! Dark-flavored truecolor palette.

/// What a token is, semantically — the theme maps these to colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Everything unclassified (identifiers, punctuation).
    Plain,
    /// Language keywords (`fn`, `def`, `SELECT`, …).
    Keyword,
    /// Numbers, named constants (`42`, `true`, `None`), and
    /// SCREAMING_CASE identifiers.
    Literal,
    /// String contents, delimiters included.
    Str,
    /// Comments, line or block.
    Comment,
    /// An identifier being called: `name(`.
    Function,
    /// A Capitalized identifier — types, classes, constructors.
    Type,
}

struct Lang {
    names: &'static [&'static str],
    line_comments: &'static [&'static str],
    block_comment: Option<(&'static str, &'static str)>,
    /// Multi-line string forms (triple quotes, raw/template strings).
    block_strings: &'static [(&'static str, &'static str)],
    /// Single-line string delimiters (backslash-escapable).
    string_delims: &'static [char],
    keywords: &'static [&'static str],
    literals: &'static [&'static str],
    /// Compare keywords case-insensitively (SQL).
    case_insensitive: bool,
    /// Classify calls / Capitalized types / SCREAMING_CASE constants.
    /// Off for shells and data formats, where capitalization means
    /// nothing (`echo Hello` is not a type).
    rich_idents: bool,
}

const LANGS: &[Lang] = &[
    Lang {
        names: &["rust", "rs"],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        block_strings: &[],
        // No '\'' — rust single quotes are usually lifetimes.
        string_delims: &['"'],
        keywords: &[
            "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum",
            "extern", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move",
            "mut", "pub", "ref", "return", "static", "struct", "super", "trait", "type", "unsafe",
            "use", "where", "while",
        ],
        literals: &["Err", "None", "Ok", "Self", "Some", "false", "self", "true"],
        case_insensitive: false,
        rich_idents: true,
    },
    Lang {
        names: &["python", "py"],
        line_comments: &["#"],
        block_comment: None,
        block_strings: &[("\"\"\"", "\"\"\""), ("'''", "'''")],
        string_delims: &['"', '\''],
        keywords: &[
            "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del",
            "elif", "else", "except", "finally", "for", "from", "global", "if", "import", "in",
            "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return", "try", "while",
            "with", "yield",
        ],
        literals: &["False", "None", "True", "self"],
        case_insensitive: false,
        rich_idents: true,
    },
    Lang {
        names: &["javascript", "js", "typescript", "ts", "jsx", "tsx"],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        block_strings: &[("`", "`")],
        string_delims: &['"', '\''],
        keywords: &[
            "abstract",
            "as",
            "async",
            "await",
            "break",
            "case",
            "catch",
            "class",
            "const",
            "continue",
            "declare",
            "default",
            "do",
            "else",
            "enum",
            "export",
            "extends",
            "finally",
            "for",
            "from",
            "function",
            "get",
            "if",
            "implements",
            "import",
            "in",
            "instanceof",
            "interface",
            "let",
            "namespace",
            "new",
            "of",
            "private",
            "protected",
            "public",
            "readonly",
            "return",
            "set",
            "static",
            "switch",
            "throw",
            "try",
            "type",
            "typeof",
            "var",
            "while",
            "yield",
        ],
        literals: &["NaN", "false", "null", "super", "this", "true", "undefined"],
        case_insensitive: false,
        rich_idents: true,
    },
    Lang {
        names: &["sh", "bash", "shell", "zsh"],
        line_comments: &["#"],
        block_comment: None,
        block_strings: &[],
        string_delims: &['"', '\''],
        keywords: &[
            "case", "declare", "do", "done", "echo", "elif", "else", "esac", "exit", "export",
            "fi", "for", "function", "if", "in", "local", "read", "readonly", "return", "set",
            "shift", "source", "then", "unset", "until", "while",
        ],
        literals: &["false", "true"],
        case_insensitive: false,
        rich_idents: false,
    },
    Lang {
        names: &["go", "golang"],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        block_strings: &[("`", "`")],
        string_delims: &['"', '\''],
        keywords: &[
            "break",
            "case",
            "chan",
            "const",
            "continue",
            "default",
            "defer",
            "else",
            "fallthrough",
            "for",
            "func",
            "go",
            "goto",
            "if",
            "import",
            "interface",
            "map",
            "package",
            "range",
            "return",
            "select",
            "struct",
            "switch",
            "type",
            "var",
        ],
        literals: &["false", "iota", "nil", "true"],
        case_insensitive: false,
        rich_idents: true,
    },
    Lang {
        names: &["c", "cpp", "c++", "cc", "h", "hpp"],
        line_comments: &["//"],
        block_comment: Some(("/*", "*/")),
        block_strings: &[],
        string_delims: &['"', '\''],
        keywords: &[
            "auto",
            "break",
            "case",
            "char",
            "class",
            "const",
            "continue",
            "default",
            "delete",
            "do",
            "double",
            "else",
            "enum",
            "extern",
            "float",
            "for",
            "goto",
            "if",
            "inline",
            "int",
            "long",
            "namespace",
            "new",
            "override",
            "private",
            "protected",
            "public",
            "return",
            "short",
            "signed",
            "sizeof",
            "static",
            "struct",
            "switch",
            "template",
            "typedef",
            "typename",
            "union",
            "unsigned",
            "using",
            "virtual",
            "void",
            "volatile",
            "while",
        ],
        literals: &["NULL", "false", "nullptr", "true"],
        case_insensitive: false,
        rich_idents: true,
    },
    Lang {
        names: &["json"],
        line_comments: &[],
        block_comment: None,
        block_strings: &[],
        string_delims: &['"'],
        keywords: &[],
        literals: &["false", "null", "true"],
        case_insensitive: false,
        rich_idents: false,
    },
    Lang {
        names: &["yaml", "yml"],
        line_comments: &["#"],
        block_comment: None,
        block_strings: &[],
        string_delims: &['"', '\''],
        keywords: &[],
        literals: &["false", "no", "null", "true", "yes"],
        case_insensitive: false,
        rich_idents: false,
    },
    Lang {
        names: &["toml"],
        line_comments: &["#"],
        block_comment: None,
        block_strings: &[("\"\"\"", "\"\"\"")],
        string_delims: &['"', '\''],
        keywords: &[],
        literals: &["false", "true"],
        case_insensitive: false,
        rich_idents: false,
    },
    Lang {
        names: &["sql"],
        line_comments: &["--"],
        block_comment: Some(("/*", "*/")),
        block_strings: &[],
        string_delims: &['\''],
        keywords: &[
            "alter", "and", "as", "asc", "by", "create", "delete", "desc", "distinct", "drop",
            "from", "group", "having", "index", "inner", "insert", "into", "join", "left", "limit",
            "not", "on", "or", "order", "outer", "primary", "right", "select", "set", "table",
            "union", "update", "values", "where",
        ],
        literals: &["false", "null", "true"],
        case_insensitive: true,
        rich_idents: false,
    },
];

/// Multi-line region carried between lines of one code block.
enum Region {
    None,
    BlockComment,
    /// Inside a block string; remembers which closer ends it.
    BlockString(&'static str),
}

/// Per-code-block tokenizer: create one per fence, feed it lines in
/// order (block comments and strings span lines).
pub struct Highlighter {
    lang: &'static Lang,
    region: Region,
}

impl Highlighter {
    /// A highlighter for the fence's info-string language, or `None`
    /// for languages we don't know (callers render plain).
    pub fn for_lang(name: &str) -> Option<Highlighter> {
        let name = name.to_ascii_lowercase();
        let lang = LANGS.iter().find(|l| l.names.contains(&name.as_str()))?;
        Some(Highlighter {
            lang,
            region: Region::None,
        })
    }

    /// Tokenize one line into `(text, kind)` runs; adjacent runs of the
    /// same kind are merged. The concatenated runs equal the input.
    pub fn line(&mut self, line: &str) -> Vec<(String, Kind)> {
        let mut out: Vec<(String, Kind)> = Vec::new();
        let mut push = |s: &str, k: Kind| {
            if s.is_empty() {
                return;
            }
            match out.last_mut() {
                Some((prev, pk)) if *pk == k => prev.push_str(s),
                _ => out.push((s.to_string(), k)),
            }
        };
        let mut i = 0;
        'outer: while i < line.len() {
            let rest = &line[i..];
            match self.region {
                Region::BlockComment => {
                    let (open, close) = self.lang.block_comment.expect("in block comment");
                    let _ = open;
                    if let Some(j) = rest.find(close) {
                        push(&rest[..j + close.len()], Kind::Comment);
                        self.region = Region::None;
                        i += j + close.len();
                    } else {
                        push(rest, Kind::Comment);
                        break;
                    }
                }
                Region::BlockString(close) => {
                    if let Some(j) = rest.find(close) {
                        push(&rest[..j + close.len()], Kind::Str);
                        self.region = Region::None;
                        i += j + close.len();
                    } else {
                        push(rest, Kind::Str);
                        break;
                    }
                }
                Region::None => {
                    for (open, close) in self.lang.block_strings {
                        if rest.starts_with(open) {
                            push(open, Kind::Str);
                            i += open.len();
                            self.region = Region::BlockString(close);
                            continue 'outer;
                        }
                    }
                    if let Some((open, _)) = self.lang.block_comment
                        && rest.starts_with(open)
                    {
                        push(open, Kind::Comment);
                        i += open.len();
                        self.region = Region::BlockComment;
                        continue;
                    }
                    if self.lang.line_comments.iter().any(|c| rest.starts_with(c)) {
                        push(rest, Kind::Comment);
                        break;
                    }
                    let ch = rest.chars().next().expect("i < len");
                    if self.lang.string_delims.contains(&ch) {
                        i += self.consume_string(rest, ch, &mut push);
                        continue;
                    }
                    if ch.is_ascii_digit() {
                        let end = rest
                            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
                            .unwrap_or(rest.len());
                        push(&rest[..end], Kind::Literal);
                        i += end;
                        continue;
                    }
                    if ch.is_alphanumeric() || ch == '_' {
                        let end = rest
                            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                            .unwrap_or(rest.len());
                        let word = &rest[..end];
                        let called = rest[end..].starts_with('(');
                        push(word, self.word_kind(word, called));
                        i += end;
                        continue;
                    }
                    push(&rest[..ch.len_utf8()], Kind::Plain);
                    i += ch.len_utf8();
                }
            }
        }
        out
    }

    /// A single-line string starting at `rest[0]` (== `delim`):
    /// backslash-escapes honored, unterminated runs color to the end of
    /// the line. Returns bytes consumed.
    fn consume_string(&self, rest: &str, delim: char, push: &mut impl FnMut(&str, Kind)) -> usize {
        let mut chars = rest.char_indices().skip(1);
        let mut escaped = false;
        for (j, c) in &mut chars {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == delim {
                let end = j + c.len_utf8();
                push(&rest[..end], Kind::Str);
                return end;
            }
        }
        push(rest, Kind::Str);
        rest.len()
    }

    /// Classify one identifier; `called` = immediately followed by `(`.
    fn word_kind(&self, word: &str, called: bool) -> Kind {
        if self.lang.case_insensitive {
            let lower = word.to_ascii_lowercase();
            if self.lang.keywords.contains(&lower.as_str()) {
                return Kind::Keyword;
            }
            if self.lang.literals.contains(&lower.as_str()) {
                return Kind::Literal;
            }
            return Kind::Plain;
        }
        if self.lang.keywords.contains(&word) {
            return Kind::Keyword;
        }
        if self.lang.literals.contains(&word) {
            return Kind::Literal;
        }
        if !self.lang.rich_idents {
            return Kind::Plain;
        }
        let mut chars = word.chars();
        let first = chars.next().expect("words are non-empty");
        // SCREAMING_CASE reads as a constant…
        if first.is_ascii_uppercase()
            && word.len() > 1
            && word
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        {
            return Kind::Literal;
        }
        // …Capitalized as a type/class/constructor…
        if first.is_uppercase() {
            return Kind::Type;
        }
        // …and anything being called as a function.
        if called {
            return Kind::Function;
        }
        Kind::Plain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(lang: &str, line: &str) -> Vec<(String, Kind)> {
        Highlighter::for_lang(lang).unwrap().line(line)
    }

    #[test]
    fn unknown_language_is_none() {
        assert!(Highlighter::for_lang("brainfuck").is_none());
        assert!(Highlighter::for_lang("").is_none());
    }

    #[test]
    fn rust_tokens() {
        let toks = kinds("rust", "fn main() { let x = 42; // answer");
        assert!(toks.contains(&("fn".into(), Kind::Keyword)));
        assert!(toks.contains(&("let".into(), Kind::Keyword)));
        assert!(toks.contains(&("42".into(), Kind::Literal)));
        assert!(toks.contains(&("// answer".into(), Kind::Comment)));
        // Runs concatenate back to the input.
        let joined: String = toks.iter().map(|(s, _)| s.as_str()).collect();
        assert_eq!(joined, "fn main() { let x = 42; // answer");
    }

    #[test]
    fn strings_with_escapes() {
        let toks = kinds("rust", r#"print("a \" b") + "unterminated"#);
        assert!(toks.contains(&(r#""a \" b""#.into(), Kind::Str)));
        assert!(toks.contains(&(r#""unterminated"#.into(), Kind::Str)));
    }

    #[test]
    fn python_triple_quote_spans_lines() {
        let mut h = Highlighter::for_lang("python").unwrap();
        let first = h.line("x = \"\"\"doc");
        // Opener and contents merge into one string run.
        assert!(first.contains(&("\"\"\"doc".into(), Kind::Str)));
        // Still a string on the next line, until the closer.
        let second = h.line("still text\"\"\" + 1");
        assert_eq!(second[0], ("still text\"\"\"".into(), Kind::Str));
        assert!(second.contains(&("1".into(), Kind::Literal)));
    }

    #[test]
    fn block_comment_spans_lines() {
        let mut h = Highlighter::for_lang("c").unwrap();
        h.line("/* start");
        let toks = h.line("end */ int x;");
        assert_eq!(toks[0], ("end */".into(), Kind::Comment));
        assert!(toks.contains(&("int".into(), Kind::Keyword)));
    }

    #[test]
    fn sql_keywords_any_case() {
        let toks = kinds("sql", "SELECT name FROM users");
        assert!(toks.contains(&("SELECT".into(), Kind::Keyword)));
        assert!(toks.contains(&("FROM".into(), Kind::Keyword)));
        assert!(
            toks.iter()
                .any(|(s, k)| s.contains("users") && *k == Kind::Plain)
        );
    }

    #[test]
    fn rich_identifiers_classify() {
        let toks = kinds("rust", "let cfg = Config::load(MAX_RETRIES)");
        assert!(toks.contains(&("Config".into(), Kind::Type)));
        assert!(toks.contains(&("load".into(), Kind::Function)));
        assert!(toks.contains(&("MAX_RETRIES".into(), Kind::Literal)));
        assert!(
            toks.iter()
                .any(|(s, k)| s.contains("cfg") && *k == Kind::Plain)
        );
    }

    #[test]
    fn data_and_shell_idents_stay_plain() {
        // `echo Hello` is not a type; yaml values aren't constructors.
        let toks = kinds("sh", "echo Hello WORLD");
        assert!(
            toks.iter()
                .all(|(_, k)| *k != Kind::Type && *k != Kind::Function)
        );
        let toks = kinds("yaml", "name: Alex(1)");
        assert!(
            toks.iter()
                .all(|(_, k)| *k != Kind::Type && *k != Kind::Function)
        );
    }

    #[test]
    fn rust_lifetimes_are_not_strings() {
        let toks = kinds("rust", "fn f<'a>(x: &'a str) {}");
        assert!(toks.iter().all(|(_, k)| *k != Kind::Str));
    }
}
