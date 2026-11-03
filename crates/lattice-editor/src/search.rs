//! `/` on the help page and the manual: finds words, as Helix's search does, scrolling to the
//! first line with them as they're typed. `n` and `N` find the next and the one before, going
//! round the end.

use crate::{Editor, Effect, Key, Mode, Page, PromptKind};

/// What `/` is looking for, and where the page was when it began, to go back to on `esc`.
#[derive(Debug, Clone, Default)]
pub(crate) struct Search {
    query: String,
    from: usize,
}

impl Editor {
    /// `/`, `n` and `N` on the pages for reading; None for any other key, or page.
    pub(crate) fn search_key(&mut self, key: Key) -> Option<Vec<Effect>> {
        let (_, scroll) = self.readable()?;
        match key {
            Key::Char('/') => {
                self.search.from = scroll;
                self.open_prompt(PromptKind::Search);
            }
            Key::Char('n') => self.find_again(scroll + 1, true),
            Key::Char('N') => self.find_again(scroll.saturating_sub(1), false),
            _ => return None,
        }
        Some(vec![])
    }

    /// Scrolls to the first line with what's typed so far, from where the search began.
    pub(crate) fn search_preview(&mut self) {
        let Mode::Command(prompt) = &self.mode else {
            return;
        };
        let query = prompt.line.to_lowercase();
        let from = self.search.from;
        let found = self.find(&query, from, true).map(|(line, _)| line);
        self.scroll_to(found.unwrap_or(from));
    }

    /// `ret`: keeps the search for `n` and `N`, and says if there's nothing to find.
    pub(crate) fn finish_search(&mut self, line: &str) {
        self.search.query = line.to_lowercase();
        if line.is_empty() {
            return;
        }
        match self.find(&self.search.query, self.search.from, true) {
            Some((found, wrapped)) => {
                self.scroll_to(found);
                if wrapped {
                    self.message = Some("round to the top".into());
                }
            }
            None => {
                self.scroll_to(self.search.from);
                self.message = Some(format!("no match for {line}"));
            }
        }
    }

    /// `esc`: back to where the search began.
    pub(crate) fn cancel_search(&mut self) {
        self.scroll_to(self.search.from);
    }

    fn find_again(&mut self, from: usize, forward: bool) {
        if self.search.query.is_empty() {
            self.message = Some("nothing to find yet: / to search".into());
            return;
        }
        let query = self.search.query.clone();
        match self.find(&query, from, forward) {
            Some((found, wrapped)) => {
                self.scroll_to(found);
                if wrapped {
                    let end = if forward { "top" } else { "end" };
                    self.message = Some(format!("round to the {end}"));
                }
            }
            None => self.message = Some(format!("no match for {query}")),
        }
    }

    /// The first line with `query`, from line `from` on (or back), going round the end, and
    /// whether it went round.
    fn find(&self, query: &str, from: usize, forward: bool) -> Option<(usize, bool)> {
        let (lines, _) = self.readable()?;
        let has = |i: &usize| lines.get(*i).is_some_and(|l| l.contains(query));
        let from = from.min(lines.len());
        let found = if forward {
            (from..lines.len()).find(has).map(|i| (i, false))
        } else {
            (0..=from).rev().find(has).map(|i| (i, false))
        };
        found.or_else(|| {
            let wrapped = if forward {
                (0..from).find(has)
            } else {
                (from..lines.len()).rev().find(has)
            };
            wrapped.map(|i| (i, true))
        })
    }

    /// The lines of the page being read, in lower case, and where it's scrolled to.
    fn readable(&self) -> Option<(Vec<String>, usize)> {
        match self.page {
            Page::Help => {
                let lines = self.help_lines();
                let lines = lines.iter().map(|l| l.text().to_lowercase());
                Some((lines.collect(), self.help.scroll))
            }
            Page::Manual => {
                let lines = self.manual_lines();
                let lines = lines
                    .iter()
                    .map(|l| crate::manual::shown(l.text()).to_lowercase());
                Some((lines.collect(), self.manual.scroll))
            }
            _ => None,
        }
    }

    fn scroll_to(&mut self, line: usize) {
        match self.page {
            Page::Help => self.help.scroll = line,
            Page::Manual => self.manual.scroll = line,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::tests::editor;
    use crate::{Editor, Key, Mode};

    fn keys(e: &mut Editor, s: &str) {
        for c in s.chars() {
            e.handle(if c == '\n' { Key::Enter } else { Key::Char(c) });
        }
    }

    fn top(e: &Editor) -> String {
        e.manual_lines()[e.manual.scroll].text().to_lowercase()
    }

    #[test]
    fn slash_finds_words_in_the_manual_and_n_the_next() {
        let mut e = editor();
        keys(&mut e, ":manual\n/wicki");
        assert!(matches!(e.mode, Mode::Command(_)));
        assert!(top(&e).contains("wicki"), "scrolls as it's typed");
        e.handle(Key::Enter);
        let first = e.manual.scroll;
        keys(&mut e, "n");
        assert!(e.manual.scroll > first);
        assert!(top(&e).contains("wicki"));
        keys(&mut e, "N");
        assert_eq!(e.manual.scroll, first);
    }

    #[test]
    fn it_finds_words_as_shown_not_their_marks() {
        let mut e = editor();
        // Written `the **stops**, in order`.
        keys(&mut e, ":manual\n/the stops, in order\n");
        assert!(top(&e).contains("**stops**, in order"), "{}", top(&e));
    }

    #[test]
    fn esc_goes_back_and_a_miss_says_so() {
        let mut e = editor();
        keys(&mut e, ":manual\nj/wicki");
        assert_ne!(e.manual.scroll, 1);
        e.handle(Key::Esc);
        assert_eq!(e.manual.scroll, 1);
        keys(&mut e, "/zzyzx\n");
        assert_eq!(e.manual.scroll, 1);
        assert_eq!(e.message.as_deref(), Some("no match for zzyzx"));
    }

    #[test]
    fn the_help_page_finds_keys_too() {
        let mut e = editor();
        keys(&mut e, ":help\n/quit\n");
        let lines = e.help_lines();
        let line = lines[e.help.scroll].text().to_lowercase();
        assert!(line.contains("quit"), "{line}");
    }
}
