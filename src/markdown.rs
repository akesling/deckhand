//! Markdown → ratatui `Text` renderer with word wrapping.

use pulldown_cmark::{Alignment as MdAlign, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;

pub fn render(md: &str, width: u16, theme: &Theme) -> Text<'static> {
    let width = (width as usize).max(10);
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_TASKLISTS);
    let mut r = Renderer::new(width, theme);
    for ev in Parser::new_ext(md, opts) {
        r.event(ev);
    }
    r.finish()
}

struct TableAcc {
    aligns: Vec<MdAlign>,
    header: Vec<String>,
    rows: Vec<Vec<String>>,
    cur_row: Vec<String>,
    cur_cell: String,
}

struct Renderer<'t> {
    theme: &'t Theme,
    width: usize,
    lines: Vec<Line<'static>>,
    cur: Vec<Span<'static>>,
    cur_w: usize,
    pending_space: bool,
    /// Continuation indent (spaces) for wrapped lines, excluding quote prefix.
    wrap_indent: usize,
    styles: Vec<Style>,
    list_stack: Vec<Option<u64>>,
    quote_depth: usize,
    in_code: bool,
    link_url: Option<String>,
    suppress_blank: bool,
    table: Option<TableAcc>,
}

impl<'t> Renderer<'t> {
    fn new(width: usize, theme: &'t Theme) -> Self {
        Renderer {
            theme,
            width,
            lines: Vec::new(),
            cur: Vec::new(),
            cur_w: 0,
            pending_space: false,
            wrap_indent: 0,
            styles: Vec::new(),
            list_stack: Vec::new(),
            quote_depth: 0,
            in_code: false,
            link_url: None,
            suppress_blank: false,
            table: None,
        }
    }

    fn style(&self) -> Style {
        self.styles
            .iter()
            .fold(Style::default(), |acc, s| acc.patch(*s))
    }

    fn prefix_width(&self) -> usize {
        self.quote_depth * 2 + self.wrap_indent
    }

    /// Push raw text into the current line, adding the line prefix (quote
    /// marker + indent) if the line is empty. No wrapping.
    fn push_raw(&mut self, s: &str, style: Style) {
        if self.cur.is_empty() {
            if self.quote_depth > 0 {
                self.cur.push(Span::styled(
                    "▌ ".repeat(self.quote_depth),
                    Style::default().fg(self.theme.quote),
                ));
            }
            if self.wrap_indent > 0 {
                self.cur.push(Span::raw(" ".repeat(self.wrap_indent)));
            }
            self.cur_w = self.prefix_width();
        }
        self.cur_w += s.width();
        if let Some(last) = self.cur.last_mut()
            && last.style == style
        {
            last.content.to_mut().push_str(s);
            return;
        }
        self.cur.push(Span::styled(s.to_string(), style));
    }

    fn flush(&mut self) {
        if !self.cur.is_empty() {
            let spans = std::mem::take(&mut self.cur);
            self.lines.push(Line::from(spans));
        }
        self.cur_w = 0;
        self.pending_space = false;
    }

    fn blank(&mut self) {
        self.flush();
        if self.suppress_blank {
            self.suppress_blank = false;
            return;
        }
        if self.lines.last().map(|l| l.width() > 0).unwrap_or(false) {
            self.lines.push(Line::default());
        }
    }

    fn push_word(&mut self, word: &str) {
        let style = self.style();
        let ww = word.width();
        let sep = if self.pending_space && !self.cur.is_empty() {
            1
        } else {
            0
        };
        if !self.cur.is_empty() && self.cur_w + sep + ww > self.width {
            self.flush();
        }
        if self.pending_space && !self.cur.is_empty() {
            self.push_raw(" ", style);
        }
        self.pending_space = false;
        // Hard-split words wider than the content width.
        let avail = self.width.saturating_sub(self.prefix_width()).max(1);
        if ww > avail {
            let mut chunk = String::new();
            let mut w = 0usize;
            for ch in word.chars() {
                let cw = UnicodeWidthStr::width(ch.to_string().as_str());
                if w + cw > avail && !chunk.is_empty() {
                    self.push_raw(&chunk, style);
                    self.flush();
                    chunk.clear();
                    w = 0;
                }
                chunk.push(ch);
                w += cw;
            }
            if !chunk.is_empty() {
                self.push_raw(&chunk, style);
            }
            return;
        }
        self.push_raw(word, style);
    }

    fn push_words(&mut self, text: &str) {
        if text.starts_with(char::is_whitespace) {
            self.pending_space = true;
        }
        let mut first = true;
        for word in text.split_whitespace() {
            if !first {
                self.pending_space = true;
            }
            self.push_word(word);
            first = false;
        }
        if text.ends_with(char::is_whitespace) {
            self.pending_space = true;
        }
    }

    fn event(&mut self, ev: Event) {
        if self.table.is_some() {
            self.table_event(ev);
            return;
        }
        match ev {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => {
                if self.in_code {
                    self.code_text(&t);
                } else {
                    self.push_words(&t);
                }
            }
            Event::Code(t) => {
                let style = self
                    .style()
                    .patch(Style::default().fg(self.theme.inline_code));
                let sep = if self.pending_space && !self.cur.is_empty() {
                    1
                } else {
                    0
                };
                if !self.cur.is_empty() && self.cur_w + sep + t.width() > self.width {
                    self.flush();
                }
                if self.pending_space && !self.cur.is_empty() {
                    self.push_raw(" ", self.style());
                }
                self.pending_space = false;
                self.push_raw(&t, style);
            }
            Event::SoftBreak => self.pending_space = true,
            Event::HardBreak => self.flush(),
            Event::Rule => {
                self.blank();
                self.lines.push(Line::from(Span::styled(
                    "─".repeat(self.width),
                    Style::default().fg(self.theme.muted),
                )));
                self.suppress_blank = false;
            }
            Event::TaskListMarker(done) => {
                let mark = if done { "✓ " } else { "☐ " };
                let color = if done { Color::Green } else { self.theme.muted };
                self.push_raw(mark, Style::default().fg(color));
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Paragraph => self.blank(),
            Tag::Heading { level, .. } => {
                self.blank();
                let style = match level {
                    HeadingLevel::H1 => Style::default()
                        .fg(self.theme.h1)
                        .add_modifier(Modifier::BOLD),
                    HeadingLevel::H2 => Style::default()
                        .fg(self.theme.h2)
                        .add_modifier(Modifier::BOLD),
                    HeadingLevel::H3 => Style::default().add_modifier(Modifier::BOLD),
                    _ => Style::default()
                        .add_modifier(Modifier::BOLD)
                        .add_modifier(Modifier::DIM),
                };
                self.styles.push(style);
            }
            Tag::BlockQuote(_) => {
                self.blank();
                self.quote_depth += 1;
                self.styles
                    .push(Style::default().add_modifier(Modifier::ITALIC));
            }
            Tag::CodeBlock(kind) => {
                self.blank();
                self.in_code = true;
                let _ = kind; // no syntax highlighting (yet)
            }
            Tag::List(start) => {
                if self.list_stack.is_empty() {
                    self.blank();
                } else {
                    self.flush();
                }
                self.list_stack.push(start);
            }
            Tag::Item => {
                self.flush();
                let depth = self.list_stack.len().saturating_sub(1);
                self.wrap_indent = depth * 3;
                let bullet = match self.list_stack.last_mut() {
                    Some(Some(n)) => {
                        let b = format!("{n}. ");
                        *n += 1;
                        b
                    }
                    _ => {
                        let glyph = ["•", "◦", "▪"][depth.min(2)];
                        format!("{glyph} ")
                    }
                };
                let bw = bullet.width();
                self.push_raw(&bullet, Style::default().fg(self.theme.bullet));
                self.wrap_indent += bw;
                self.suppress_blank = true;
            }
            Tag::Emphasis => self
                .styles
                .push(Style::default().add_modifier(Modifier::ITALIC)),
            Tag::Strong => self
                .styles
                .push(Style::default().add_modifier(Modifier::BOLD)),
            Tag::Strikethrough => self
                .styles
                .push(Style::default().add_modifier(Modifier::CROSSED_OUT)),
            Tag::Link { dest_url, .. } => {
                self.link_url = Some(dest_url.to_string());
                self.styles.push(
                    Style::default()
                        .fg(self.theme.link)
                        .add_modifier(Modifier::UNDERLINED),
                );
            }
            Tag::Image { .. } => {
                self.styles.push(
                    Style::default()
                        .add_modifier(Modifier::DIM)
                        .add_modifier(Modifier::ITALIC),
                );
                self.push_words("[image: ");
            }
            Tag::Table(aligns) => {
                self.blank();
                self.table = Some(TableAcc {
                    aligns,
                    header: Vec::new(),
                    rows: Vec::new(),
                    cur_row: Vec::new(),
                    cur_cell: String::new(),
                });
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush(),
            TagEnd::Heading(level) => {
                let heading_w = self.cur_w.min(self.width);
                self.flush();
                self.styles.pop();
                if level == HeadingLevel::H1 && heading_w > 0 {
                    self.lines.push(Line::from(Span::styled(
                        "─".repeat(heading_w.max(4)),
                        Style::default().fg(self.theme.muted),
                    )));
                }
            }
            TagEnd::BlockQuote(..) => {
                self.flush();
                self.quote_depth = self.quote_depth.saturating_sub(1);
                self.styles.pop();
            }
            TagEnd::CodeBlock => {
                self.in_code = false;
            }
            TagEnd::List(_) => {
                self.flush();
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.wrap_indent = 0;
                }
            }
            TagEnd::Item => {
                self.flush();
                let depth = self.list_stack.len().saturating_sub(1);
                self.wrap_indent = depth * 3;
            }
            TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                self.styles.pop();
            }
            TagEnd::Link => {
                self.styles.pop();
                if let Some(url) = self.link_url.take()
                    && !url.starts_with('#')
                {
                    self.styles.push(
                        Style::default()
                            .fg(self.theme.muted)
                            .add_modifier(Modifier::DIM),
                    );
                    self.push_words(&format!(" ({url})"));
                    self.styles.pop();
                }
            }
            TagEnd::Image => {
                self.push_words("]");
                self.styles.pop();
            }
            _ => {}
        }
    }

    fn code_text(&mut self, text: &str) {
        self.flush();
        let style = Style::default()
            .bg(self.theme.code_bg)
            .fg(self.theme.code_fg);
        let mut lines: Vec<&str> = text.split('\n').collect();
        if lines.last() == Some(&"") {
            lines.pop();
        }
        for l in lines {
            let mut content = format!("  {l}");
            let w = content.width();
            if w < self.width {
                content.push_str(&" ".repeat(self.width - w));
            }
            self.lines.push(Line::from(Span::styled(content, style)));
        }
    }

    fn table_event(&mut self, ev: Event) {
        let t = self.table.as_mut().expect("in table");
        match ev {
            Event::Start(Tag::TableHead) | Event::Start(Tag::TableRow) => t.cur_row.clear(),
            Event::Start(Tag::TableCell) => t.cur_cell.clear(),
            Event::End(TagEnd::TableCell) => {
                let cell = std::mem::take(&mut t.cur_cell);
                t.cur_row.push(cell.trim().to_string());
            }
            Event::End(TagEnd::TableHead) => t.header = std::mem::take(&mut t.cur_row),
            Event::End(TagEnd::TableRow) => {
                let row = std::mem::take(&mut t.cur_row);
                t.rows.push(row);
            }
            Event::End(TagEnd::Table) => {
                let acc = self.table.take().expect("in table");
                self.render_table(acc);
            }
            Event::Text(s) | Event::Code(s) => t.cur_cell.push_str(&s),
            Event::SoftBreak | Event::HardBreak => t.cur_cell.push(' '),
            _ => {}
        }
    }

    fn render_table(&mut self, acc: TableAcc) {
        let ncols = acc
            .header
            .len()
            .max(acc.rows.iter().map(|r| r.len()).max().unwrap_or(0));
        if ncols == 0 {
            return;
        }
        let mut widths = vec![0usize; ncols];
        for row in std::iter::once(&acc.header).chain(acc.rows.iter()) {
            for (i, cell) in row.iter().enumerate() {
                widths[i] = widths[i].max(cell.width()).min(40);
            }
        }
        let pad = |cell: &str, i: usize| -> String {
            let w = widths[i];
            let cw = cell.width().min(w);
            let cell: String = {
                // truncate to column width
                let mut out = String::new();
                let mut acc_w = 0;
                for ch in cell.chars() {
                    let chw = UnicodeWidthStr::width(ch.to_string().as_str());
                    if acc_w + chw > w {
                        break;
                    }
                    out.push(ch);
                    acc_w += chw;
                }
                out
            };
            let fill = w - cw;
            match acc.aligns.get(i) {
                Some(MdAlign::Right) => format!("{}{}", " ".repeat(fill), cell),
                Some(MdAlign::Center) => {
                    let l = fill / 2;
                    format!("{}{}{}", " ".repeat(l), cell, " ".repeat(fill - l))
                }
                _ => format!("{}{}", cell, " ".repeat(fill)),
            }
        };

        if !acc.header.is_empty() {
            let cells: Vec<String> = (0..ncols)
                .map(|i| pad(acc.header.get(i).map(|s| s.as_str()).unwrap_or(""), i))
                .collect();
            self.lines.push(Line::from(Span::styled(
                cells.join("  "),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            let sep: Vec<String> = widths.iter().map(|w| "─".repeat(*w)).collect();
            self.lines.push(Line::from(Span::styled(
                sep.join("──"),
                Style::default().fg(self.theme.muted),
            )));
        }
        for row in &acc.rows {
            let cells: Vec<String> = (0..ncols)
                .map(|i| pad(row.get(i).map(|s| s.as_str()).unwrap_or(""), i))
                .collect();
            self.lines.push(Line::from(cells.join("  ")));
        }
    }

    fn finish(mut self) -> Text<'static> {
        self.flush();
        while self.lines.first().map(|l| l.width() == 0).unwrap_or(false) {
            self.lines.remove(0);
        }
        while self.lines.last().map(|l| l.width() == 0).unwrap_or(false) {
            self.lines.pop();
        }
        Text::from(self.lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(md: &str, width: u16) -> Text<'static> {
        render(md, width, &Theme::default())
    }

    #[test]
    fn renders_and_wraps() {
        let text = r("hello world this is a somewhat long paragraph of text", 20);
        assert!(text.height() > 1);
        for line in &text.lines {
            assert!(line.width() <= 20, "line too wide: {:?}", line);
        }
    }

    #[test]
    fn heading_gets_underline() {
        let text = r("# Title", 40);
        assert_eq!(text.height(), 2);
    }

    #[test]
    fn code_block_lines_preserved() {
        let text = r("```\nfn main() {}\nlet x = 1;\n```", 40);
        assert_eq!(text.height(), 2);
    }

    #[test]
    fn list_bullets() {
        let text = r("- one\n- two\n", 40);
        assert_eq!(text.height(), 2);
        assert!(format!("{:?}", text.lines[0]).contains('•'));
    }

    #[test]
    fn themed_heading_color() {
        let theme = Theme {
            h1: Color::Magenta,
            ..Theme::default()
        };
        let text = render("# Title", 40, &theme);
        let styles: Vec<_> = text.lines[0].spans.iter().map(|s| s.style.fg).collect();
        assert!(styles.contains(&Some(Color::Magenta)), "spans: {styles:?}");
    }
}
