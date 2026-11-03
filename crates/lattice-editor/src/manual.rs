//! The manual (`:manual`): everything Lattice does, explained in full. It's written in Markdown
//! in `docs/manual`, which reads as well on GitHub, and built in here, a line of the source to a
//! line of the page, so it's kept wrapped to 100 columns.

use std::sync::OnceLock;

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

/// Where the manual is scrolled to, and the page to go back to.
#[derive(Debug, Clone, Copy, Default)]
pub struct Manual {
    pub scroll: usize,
    back: Page,
}

/// The contents, then every chapter, numbered.
pub fn lines() -> &'static [ManualLine] {
    static LINES: OnceLock<Vec<ManualLine>> = OnceLock::new();
    LINES.get_or_init(|| {
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
            for line in chapter.lines() {
                if line.starts_with("```") {
                    code = !code;
                    continue;
                }
                if code {
                    body.push(ManualLine::Code(line.to_string()));
                    continue;
                }
                let level = line.chars().take_while(|&c| c == '#').count();
                let Some(title) = line.get(level..).and_then(|t| t.strip_prefix(' ')) else {
                    // Markdown's table rules are for GitHub; the rows line up without them.
                    if !is_table_rule(line) {
                        body.push(ManualLine::Text(line.to_string()));
                    }
                    continue;
                };
                if !(1..=3).contains(&level) {
                    body.push(ManualLine::Text(line.to_string()));
                    continue;
                }
                // Numbered 3, 3.2, 3.2.1.
                if let Some(n) = numbers.get_mut(level - 1) {
                    *n += 1;
                }
                for n in numbers.iter_mut().skip(level) {
                    *n = 0;
                }
                let number: Vec<String> =
                    (numbers.iter().take(level)).map(usize::to_string).collect();
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
        }
        contents.push(ManualLine::Text(String::new()));
        contents.extend(body);
        contents
    })
}

/// `|---|:--:|`.
fn is_table_rule(line: &str) -> bool {
    let line = line.trim();
    line.starts_with('|') && line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '))
}

impl ManualLine {
    /// Its words, for finding things.
    fn text(&self) -> &str {
        match self {
            ManualLine::Heading { text, .. } | ManualLine::Text(text) | ManualLine::Code(text) => {
                text
            }
        }
    }
}

impl Editor {
    /// `:manual [TOPIC]`: opens the manual, coming back to this page, at the contents or at
    /// `topic`: a section about it, else where it's a command, else anywhere it's mentioned.
    /// Again with the same topic, it goes on to the next place.
    pub(crate) fn open_manual(&mut self, topic: &str) -> Result<(), String> {
        let topic = topic.trim().to_lowercase();
        let lines = lines();
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
        let lines = lines();
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
            | Action::Quit
            | Action::GotoMixer
            | Action::GotoPattern
            | Action::GotoSong
            | Action::GotoInstrument
            | Action::GotoEffects => return None,
            _ => {}
        }
        Some(vec![])
    }
}

fn contains(line: &ManualLine, lower: &str) -> bool {
    line.text().to_lowercase().contains(lower)
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
