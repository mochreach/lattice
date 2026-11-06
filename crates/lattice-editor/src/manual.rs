//! The manual (`:manual`): everything Lattice does, explained in full. It's written in Markdown
//! in `docs/manual`, which reads as well on GitHub, and built in here, its paragraphs and tables
//! laid out again to `MEASURE` columns, so it reads on half a laptop's screen.

use std::sync::{Arc, Mutex};

use crate::keymap::Action;
use crate::{Editor, Effect, Mode, Page};

/// The chapters, in order.
pub const CHAPTERS: &[&str] = &[
    include_str!("../../../docs/manual/01-introduction.md"),
    include_str!("../../../docs/manual/02-patterns.md"),
    include_str!("../../../docs/manual/03-tracks.md"),
    include_str!("../../../docs/manual/04-rules.md"),
    include_str!("../../../docs/manual/05-gestures.md"),
    include_str!("../../../docs/manual/06-song.md"),
    include_str!("../../../docs/manual/07-instruments.md"),
    include_str!("../../../docs/manual/08-mixer-and-effects.md"),
    include_str!("../../../docs/manual/09-recording.md"),
    include_str!("../../../docs/manual/10-files-and-config.md"),
];

/// The widest the page's prose and tables are, not counting Markdown's marks.
pub const MEASURE: usize = 80;

/// A line of the manual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManualLine {
    /// `#` a chapter, `##` a section, `###` within one; numbered as they're shown.
    Heading { level: u8, text: String },
    /// Prose, a list item or a table row, with Markdown's `code`, **bold** and links in it.
    Text(String),
    /// A line of a code block, as it is.
    Code(String),
}

/// Where the manual is scrolled to, how wide it's laid out, and the page to go back to.
#[derive(Debug, Clone, Copy)]
pub struct Manual {
    pub scroll: usize,
    measure: usize,
    back: Page,
}

impl Default for Manual {
    fn default() -> Self {
        Manual {
            scroll: 0,
            measure: MEASURE,
            back: Page::default(),
        }
    }
}

/// The contents, then every chapter, numbered, laid out to `MEASURE`.
#[cfg(test)]
fn lines() -> &'static [ManualLine] {
    static LINES: std::sync::OnceLock<Vec<ManualLine>> = std::sync::OnceLock::new();
    LINES.get_or_init(|| lay_out_manual(MEASURE))
}

/// The manual laid out to `measure` columns: `MEASURE`, or narrower on a narrower screen. The
/// last width asked for is kept, as the page asks for it on every frame.
pub fn lines_at(measure: usize) -> Arc<Vec<ManualLine>> {
    static LAST: Mutex<Option<(usize, Arc<Vec<ManualLine>>)>> = Mutex::new(None);
    let Ok(mut last) = LAST.lock() else {
        // Only a panic elsewhere while laying out could poison it; lay out afresh.
        return Arc::new(lay_out_manual(measure));
    };
    match &*last {
        Some((m, lines)) if *m == measure => Arc::clone(lines),
        _ => {
            let lines = Arc::new(lay_out_manual(measure));
            *last = Some((measure, Arc::clone(&lines)));
            lines
        }
    }
}

fn lay_out_manual(measure: usize) -> Vec<ManualLine> {
    let mut body = vec![];
    let mut contents = vec![
        ManualLine::Heading {
            level: 1,
            text: "Contents".into(),
        },
        ManualLine::Text(String::new()),
    ];
    let mut numbers = [0usize; 3];
    for chapter in CHAPTERS {
        let mut code = false;
        // The prose or table being gathered, laid out when it ends.
        let mut para: Vec<&str> = vec![];
        let mut table: Vec<&str> = vec![];
        for line in chapter.lines() {
            let is_table = !code && line.trim_start().starts_with('|');
            // As in CommonMark, only `1.` can start a list in the middle of a paragraph, so a
            // line that happens to start `15.` goes on with it.
            let item = list_lead(line).is_some_and(|lead| {
                let lead = lead.trim_start();
                para.is_empty()
                    || !lead.starts_with(|c: char| c.is_ascii_digit())
                    || lead.starts_with("1.")
            });
            let continues = !code
                && !line.trim().is_empty()
                && !is_table
                && !line.starts_with('#')
                && !item
                && para.first().is_some_and(|first| {
                    // A list item's lines are indented under it; a paragraph's aren't.
                    let indented = line.starts_with(' ');
                    indented == list_lead(first).is_some()
                });
            if !continues {
                body.extend(paragraph(&para, measure));
                para.clear();
            }
            if !is_table {
                body.extend(lay_out(&table, measure));
                table.clear();
            }
            if line.starts_with("```") {
                code = !code;
                continue;
            }
            if code {
                body.push(ManualLine::Code(line.to_string()));
                continue;
            }
            if is_table {
                table.push(line);
                continue;
            }
            let level = line.chars().take_while(|&c| c == '#').count();
            let Some(title) = line.get(level..).and_then(|t| t.strip_prefix(' ')) else {
                if line.trim().is_empty() {
                    body.push(ManualLine::Text(String::new()));
                } else {
                    para.push(line);
                }
                continue;
            };
            if !(1..=3).contains(&level) {
                para.push(line);
                continue;
            }
            // Numbered 3, 3.2, 3.2.1.
            if let Some(n) = numbers.get_mut(level - 1) {
                *n += 1;
            }
            for n in numbers.iter_mut().skip(level) {
                *n = 0;
            }
            let number: Vec<String> = (numbers.iter().take(level)).map(usize::to_string).collect();
            // Headings aren't styled within, so they go without Markdown's marks.
            let title = title.replace(['`', '*'], "");
            let text = format!("{:<6}{title}", number.join("."));
            if level <= 2 {
                let indent = "    ".repeat(level - 1);
                contents.push(ManualLine::Text(format!("{indent}{text}")));
            }
            body.push(ManualLine::Heading {
                level: level as u8,
                text,
            });
        }
        body.extend(paragraph(&para, measure));
        body.extend(lay_out(&table, measure));
    }
    contents.push(ManualLine::Text(String::new()));
    contents.extend(body);
    contents
}

/// `|---|:--:|`.
fn is_table_rule(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('|') && line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

/// The start of a list item, `- `, `* ` or `3. ` and any indent before it.
fn list_lead(line: &str) -> Option<&str> {
    let text = line.trim_start();
    let marker = if text.starts_with("- ") || text.starts_with("* ") {
        1
    } else {
        let digits = text.chars().take_while(char::is_ascii_digit).count();
        (digits > 0 && text.get(digits..)?.starts_with(". ")).then_some(digits + 1)?
    };
    let spaces = text.get(marker..)?;
    let lead = line.len() - spaces.trim_start().len();
    line.get(..lead)
}

/// A paragraph or list item, its source lines joined and wrapped again to the measure, a list
/// item's later lines hung under its text.
fn paragraph(lines: &[&str], measure: usize) -> Vec<ManualLine> {
    let Some(first) = lines.first() else {
        return vec![];
    };
    let lead = list_lead(first).unwrap_or("");
    let joined = (lines.iter().map(|l| l.trim()))
        .collect::<Vec<_>>()
        .join(" ");
    let text = joined.get(lead.trim_start().len()..).unwrap_or(&joined);
    let hang = " ".repeat(lead.chars().count());
    (wrap(text, measure.saturating_sub(hang.len()))
        .into_iter()
        .enumerate())
    .map(|(i, row)| ManualLine::Text(format!("{}{row}", if i == 0 { lead } else { &hang })))
    .collect()
}

/// A table, its columns lined up as they're shown (without Markdown's marks) and its last
/// column's cells wrapped if it's wider than the measure.
fn lay_out(lines: &[&str], measure: usize) -> Vec<ManualLine> {
    let rows: Vec<Vec<&str>> = (lines.iter())
        .filter(|l| !is_table_rule(l))
        .map(|l| {
            let l = l.trim();
            let l = l.strip_prefix('|').unwrap_or(l);
            let l = l.strip_suffix('|').unwrap_or(l);
            split(l, '|').into_iter().map(str::trim).collect()
        })
        .collect();
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let column = |c: usize| rows.iter().filter_map(move |r| r.get(c));
    let mut widths: Vec<usize> = (0..columns)
        .map(|c| column(c).map(|cell| width(cell)).max().unwrap_or(0))
        .collect();
    // Each column can narrow as far as its longest word.
    let least: Vec<usize> = (0..columns)
        .map(|c| {
            (column(c).flat_map(|cell| split(cell, ' ')))
                .map(width)
                .max()
                .unwrap_or(0)
        })
        .collect();
    // `| a | b |`: a bar and a space before each column, a space after, and a bar to end.
    while widths.iter().map(|w| w + 3).sum::<usize>() + 1 > measure {
        // The widest column that can narrow does.
        let Some(widest) = (widths.iter_mut().zip(&least))
            .filter(|(w, least)| **w > **least)
            .map(|(w, _)| w)
            .max_by_key(|w| **w)
        else {
            break;
        };
        *widest -= 1;
    }
    let mut out = vec![];
    for row in rows {
        let cells: Vec<Vec<String>> = (widths.iter().enumerate())
            .map(|(c, &w)| wrap(row.get(c).copied().unwrap_or(""), w))
            .collect();
        let height = cells.iter().map(Vec::len).max().unwrap_or(1);
        for i in 0..height {
            let mut line = String::from("|");
            for (cell, &w) in cells.iter().zip(&widths) {
                let words = cell.get(i).map(String::as_str).unwrap_or("");
                let pad = w.saturating_sub(width(words));
                line.push_str(&format!(" {words}{} |", " ".repeat(pad)));
            }
            out.push(ManualLine::Text(line));
        }
    }
    out
}

/// A span of Markdown at the start of `rest`, `code`, **bold** or [a link](to): its length in
/// the source and the words it shows. The page styles them the same way.
fn span(rest: &str) -> Option<(usize, &str)> {
    if let Some(after) = rest.strip_prefix('`') {
        let end = after.find('`')?;
        return Some((end + 2, after.get(..end)?));
    }
    if let Some(after) = rest.strip_prefix("**") {
        let end = after.find("**")?;
        return Some((end + 4, after.get(..end)?));
    }
    let after = rest.strip_prefix('[')?;
    let close = after.find("](")?;
    let end = after.get(close..)?.find(')')?;
    Some((close + end + 2, after.get(..close)?))
}

/// `text` split at `at`, but not within a span.
fn split(text: &str, at: char) -> Vec<&str> {
    let mut parts = vec![];
    let (mut start, mut i) = (0, 0);
    while let Some(rest) = text.get(i..)
        && let Some(c) = rest.chars().next()
    {
        if let Some((len, _)) = span(rest) {
            i += len;
        } else if c == at {
            parts.push(text.get(start..i).unwrap_or(""));
            i += c.len_utf8();
            start = i;
        } else {
            i += c.len_utf8();
        }
    }
    parts.push(text.get(start..).unwrap_or(""));
    parts
}

/// `text` as the page shows it, without Markdown's marks, for finding words in.
pub(crate) fn shown(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some((len, words)) = span(rest) {
            out.push_str(words);
            rest = rest.get(len..).unwrap_or("");
        } else {
            out.push(c);
            rest = rest.get(c.len_utf8()..).unwrap_or("");
        }
    }
    out
}

/// How wide `text` is on the page, without its marks.
pub fn width(text: &str) -> usize {
    let mut n = 0;
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some((len, words)) = span(rest) {
            n += words.chars().count();
            rest = rest.get(len..).unwrap_or("");
        } else {
            n += 1;
            rest = rest.get(c.len_utf8()..).unwrap_or("");
        }
    }
    n
}

/// `text` wrapped between words to `room` columns as shown, never within `code` or a link. Bold
/// words are bolded one by one, so their rows are bold too.
pub fn wrap(text: &str, room: usize) -> Vec<String> {
    let mut rows = vec![];
    let mut row = String::new();
    let mut used = 0;
    let words = split(text, ' ').into_iter().flat_map(|word| {
        let bold = (word.strip_prefix("**"))
            .and_then(|w| w.find("**").map(|end| (w, end)))
            .filter(|(w, end)| w.get(..*end).is_some_and(|b| b.contains(' ')));
        match bold {
            Some((w, end)) => {
                let after = w.get(end + 2..).unwrap_or("");
                let inner: Vec<&str> = w.get(..end).unwrap_or("").split(' ').collect();
                let last = inner.len().saturating_sub(1);
                (inner.into_iter().enumerate())
                    .map(|(i, b)| format!("**{b}**{}", if i == last { after } else { "" }))
                    .collect()
            }
            None => vec![word.to_string()],
        }
    });
    for word in words.filter(|w| !w.is_empty()) {
        let word = word.as_str();
        let w = width(word);
        if used > 0 && used + 1 + w > room {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        if used > 0 {
            row.push(' ');
            used += 1;
        }
        row.push_str(word);
        used += w;
    }
    rows.push(row);
    rows
}

impl ManualLine {
    /// Its words, for finding things.
    pub(crate) fn text(&self) -> &str {
        match self {
            ManualLine::Heading { text, .. } | ManualLine::Text(text) | ManualLine::Code(text) => {
                text
            }
        }
    }
}

impl Editor {
    /// The manual, laid out as wide as it's shown.
    pub fn manual_lines(&self) -> Arc<Vec<ManualLine>> {
        lines_at(self.manual.measure)
    }

    /// Lays the manual out again for a screen `width` columns wide, if that's narrower than
    /// `MEASURE` (less two columns each side), so its lines are never wrapped twice. Where it's
    /// scrolled to stays under the same heading.
    pub fn fit_manual(&mut self, width: usize) {
        let measure = width.saturating_sub(4).clamp(20, MEASURE);
        if measure == self.manual.measure {
            return;
        }
        let heading = |l: &ManualLine| matches!(l, ManualLine::Heading { .. });
        let old = self.manual_lines();
        let at = self.manual.scroll.min(old.len().saturating_sub(1));
        let (nth, since) = match (old.iter().take(at + 1)).rposition(heading) {
            Some(h) => (old.iter().take(h).filter(|l| heading(l)).count(), at - h),
            None => (usize::MAX, at),
        };
        self.manual.measure = measure;
        let new = self.manual_lines();
        let start = if nth == usize::MAX {
            0
        } else {
            (new.iter().enumerate())
                .filter(|(_, l)| heading(l))
                .nth(nth)
                .map_or(0, |(i, _)| i)
        };
        self.manual.scroll = (start + since).min(new.len().saturating_sub(1));
    }

    /// `:manual [TOPIC]`: opens the manual, coming back to this page, at the contents or at
    /// `topic`: a section about it, else where it's a command, else anywhere it's mentioned.
    /// Again with the same topic, it goes on to the next place.
    pub(crate) fn open_manual(&mut self, topic: &str) -> Result<(), String> {
        let topic = topic.trim().to_lowercase();
        let lines = self.manual_lines();
        let mut scroll = 0;
        if !topic.is_empty() {
            let command = format!("`:{}", topic.trim_start_matches(':'));
            let found = |test: &dyn Fn(&ManualLine) -> bool| -> Vec<usize> {
                (lines.iter().enumerate())
                    .filter(|(_, l)| test(l))
                    .map(|(i, _)| i)
                    .collect()
            };
            let places = [
                found(&|l| matches!(l, ManualLine::Heading { .. }) && contains(l, &topic)),
                found(&|l| contains(l, &command)),
                found(&|l| contains(l, &topic)),
            ];
            let places = (places.into_iter())
                .find(|p| !p.is_empty())
                .ok_or_else(|| format!("nothing in the manual about {topic}"))?;
            // The next after where it's open, round to the first.
            let now = (self.page == Page::Manual).then_some(self.manual.scroll);
            scroll = (places.iter())
                .find(|&&p| now.is_some_and(|n| p > n))
                .or(places.first())
                .copied()
                .unwrap_or(0);
        }
        if self.page != Page::Manual {
            self.manual.back = self.page;
        }
        self.manual.scroll = scroll;
        self.page = Page::Manual;
        self.mode = Mode::Normal;
        Ok(())
    }

    /// The manual's keys: scrolling, going from heading to heading, and going back. Playing,
    /// the command line and the pages work as anywhere; anything else does nothing.
    pub(crate) fn manual_act(&mut self, action: Action, n: usize) -> Option<Vec<Effect>> {
        let lines = self.manual_lines();
        let last = lines.len().saturating_sub(1);
        let scroll = &mut self.manual.scroll;
        let heading = |i: &usize| matches!(lines.get(*i), Some(ManualLine::Heading { .. }));
        match action {
            Action::Up => *scroll = scroll.saturating_sub(n),
            Action::Down => *scroll = (*scroll + n).min(last),
            Action::HalfPageUp => *scroll = scroll.saturating_sub(15 * n),
            Action::HalfPageDown => *scroll = (*scroll + 15 * n).min(last),
            Action::PageUp => *scroll = scroll.saturating_sub(30 * n),
            Action::PageDown => *scroll = (*scroll + 30 * n).min(last),
            Action::FirstStep => *scroll = 0,
            Action::LastStep => *scroll = last,
            Action::NextNote => {
                for _ in 0..n {
                    *scroll = (*scroll + 1..=last).find(heading).unwrap_or(*scroll);
                }
            }
            Action::PrevNote => {
                for _ in 0..n {
                    *scroll = (0..*scroll).rev().find(heading).unwrap_or(0);
                }
            }
            Action::Escape => self.page = self.manual.back,
            Action::CommandLine
            | Action::TogglePlay
            | Action::PlaySong
            | Action::PlayFill
            | Action::LoopPedal
            | Action::Quit
            | Action::GotoMixer
            | Action::GotoPattern
            | Action::GotoSong
            | Action::GotoInstrument
            | Action::GotoEffects => return None,
            _ => self.does_nothing(action),
        }
        Some(vec![])
    }
}

fn contains(line: &ManualLine, lower: &str) -> bool {
    shown(line.text()).to_lowercase().contains(lower)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::COMMANDS;
    use lattice_core::{FxKind, InstKind, Kind};

    /// The widest a line of the source should be, so it's never wrapped on the page.
    const WIDTH: usize = 100;

    /// The text from the heading `title` (by its words, ignoring case and Markdown) to the next
    /// heading as high or higher.
    fn section<'a>(chapters: &'a str, title: &str) -> Option<&'a str> {
        let mut start = None;
        let mut level = 0;
        let mut at = 0;
        for line in chapters.split_inclusive('\n') {
            let hashes = line.chars().take_while(|&c| c == '#').count();
            if hashes > 0 {
                if let Some(s) = start
                    && hashes <= level
                {
                    return chapters.get(s..at);
                }
                if start.is_none() && line.contains(&format!("`{title}`")) {
                    start = Some(at);
                    level = hashes;
                }
            }
            at += line.len();
        }
        start.and_then(|s| chapters.get(s..))
    }

    #[test]
    fn explains_every_command_key_and_setting() {
        let all = CHAPTERS.concat();
        let mut missing = vec![];
        for spec in COMMANDS {
            if !all.contains(&format!("`:{}", spec.name)) {
                missing.push(format!("the command `:{}`", spec.name));
            }
        }
        for action in Action::ALL {
            if !all.contains(&format!("`{}`", action.name())) {
                missing.push(format!("the action `{}`, by name", action.name()));
            }
        }
        let devices = (InstKind::ALL.iter().map(|k| (k.name(), k.params())))
            .chain(FxKind::ALL.iter().map(|k| (k.name(), k.params())));
        for (name, params) in devices {
            let Some(text) = section(&all, name) else {
                missing.push(format!("a section headed with `{name}`"));
                continue;
            };
            for param in params {
                if !text.contains(&format!("`{}`", param.name)) {
                    missing.push(format!("`{}` in the section on {name}", param.name));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "the manual doesn't explain:\n{}",
            missing.join("\n")
        );
    }

    #[test]
    fn opens_at_a_topic_and_goes_back() {
        let mut e = Editor::new(
            lattice_core::Grid {
                name: "manual".into(),
                steps_per_beat: 4,
                beats_per_bar: 4,
                grouping: vec![],
                tracks: vec![lattice_core::Track::default()],
                groups: vec![],
            },
            120.0,
        );
        crate::command::run(&mut e, "manual").unwrap();
        assert_eq!((e.page, e.manual.scroll), (Page::Manual, 0));
        // `w` goes to the next heading, which is the first chapter's, after the contents.
        e.handle(crate::keymap::Key::Char('w'));
        let at = e.manual.scroll;
        assert!(lines()[at].text().starts_with("1 "), "{:?}", lines()[at]);
        crate::command::run(&mut e, "man the pattern page").unwrap();
        assert!(
            lines()[e.manual.scroll]
                .text()
                .ends_with("The pattern page")
        );
        assert!(crate::command::run(&mut e, "manual xyzzy plugh").is_err());
        e.handle(crate::keymap::Key::Esc);
        assert_eq!(e.page, Page::Pattern);
    }

    #[test]
    fn fits_a_narrow_screen_and_stays_at_its_heading() {
        let mut e = Editor::new(
            lattice_core::Grid {
                name: "manual".into(),
                steps_per_beat: 4,
                beats_per_bar: 4,
                grouping: vec![],
                tracks: vec![lattice_core::Track::default()],
                groups: vec![],
            },
            120.0,
        );
        crate::command::run(&mut e, "manual gestures and locks").unwrap();
        let title = e.manual_lines()[e.manual.scroll].text().to_string();
        e.fit_manual(60);
        let lines = e.manual_lines();
        assert_eq!(lines[e.manual.scroll].text(), title);
        for line in lines.iter() {
            if let ManualLine::Text(text) = line
                && !text.starts_with('|')
            {
                let words = split(text, ' ');
                assert!(
                    width(text) <= 56 || words.iter().filter(|w| !w.is_empty()).count() == 1,
                    "{text}"
                );
            }
        }
        // `15.` in the middle of a paragraph goes on with it, as CommonMark has it.
        assert!(
            !lines
                .iter()
                .any(|l| l.text().starts_with("15. Each setting"))
        );
        e.fit_manual(200);
        assert_eq!(e.manual_lines()[e.manual.scroll].text(), title);
    }

    #[test]
    fn closes_its_code_on_the_line_it_opens() {
        // The page styles a line at a time, so a span of code broken over two shows its marks.
        for (c, chapter) in CHAPTERS.iter().enumerate() {
            let mut block = false;
            for (l, line) in chapter.lines().enumerate() {
                if line.starts_with("```") {
                    block = !block;
                    continue;
                }
                assert!(
                    block || line.matches('`').count() % 2 == 0,
                    "chapter {}, line {}: {line}",
                    c + 1,
                    l + 1
                );
            }
        }
    }

    #[test]
    fn is_narrow_enough_not_to_wrap() {
        for (c, chapter) in CHAPTERS.iter().enumerate() {
            for (l, line) in chapter.lines().enumerate() {
                assert!(
                    line.chars().count() <= WIDTH,
                    "chapter {}, line {}: {} columns",
                    c + 1,
                    l + 1,
                    line.chars().count()
                );
            }
        }
    }

    #[test]
    fn lays_out_its_prose_and_tables_to_the_measure() {
        for line in lines() {
            let ManualLine::Text(text) = line else {
                continue;
            };
            // Only a span too long for a row of its own may go over.
            let words = split(text.trim_start_matches([' ', '-', '|']), ' ');
            assert!(
                width(text) <= MEASURE || words.iter().filter(|w| !w.is_empty()).count() == 1,
                "{} columns: {text}",
                width(text)
            );
        }
        let table = lay_out(
            &[
                "| Setting | What it does |",
                "|---------|--------------|",
                &format!(
                    "| `drift` | {} |",
                    "how far the choirs drift apart ".repeat(4)
                ),
            ],
            MEASURE,
        );
        assert!(table.len() > 2, "its last column wraps: {table:?}");
        let Some(ManualLine::Text(header)) = table.first() else {
            panic!("{table:?}")
        };
        assert_eq!(width(header), MEASURE, "{header}");
        assert_eq!(
            paragraph(&["- **one two** three", "  four"], MEASURE),
            [ManualLine::Text("- **one** **two** three four".into())]
        );
    }

    #[test]
    fn numbers_its_headings_and_lists_them_first() {
        let lines = lines();
        assert_eq!(
            lines.first(),
            Some(&ManualLine::Heading {
                level: 1,
                text: "Contents".into()
            })
        );
        assert!(lines.iter().any(|l| l.text() == "1     Introduction"));
        assert!(lines.contains(&ManualLine::Text("2     The pattern page".into())));
        assert!(
            (lines.iter()).any(|l| l.text().ends_with("the alien harpsichord (krell)")),
            "headings go without their marks"
        );
    }
}
