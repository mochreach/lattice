//! Colours. The default follows the terminal's own palette; full themes can replace every value
//! later.

use ratatui::style::{Color, Modifier, Style};

#[derive(Debug, Clone)]
pub struct Theme {
    /// Values and other main text.
    pub text: Style,
    /// Labels, hints and secondary information.
    pub dim: Style,
    /// Empty steps, off-beat step numbers and other scaffolding.
    pub faint: Style,
    /// The logo, insert mode and messages.
    pub accent: Color,
    /// The transport while playing.
    pub playing: Color,
    /// The cursor and badge in visual mode.
    pub visual: Color,
    /// Background of selected steps.
    pub selection: Color,
    /// Background of rows on the beat, or `None` to leave them unshaded.
    pub beat_row: Option<Color>,
    /// Track colours, used in turn.
    pub tracks: Vec<Color>,
}

impl Theme {
    /// The terminal's 16-colour palette and default foreground and background, so Lattice
    /// matches whatever theme the terminal uses.
    pub fn terminal() -> Self {
        Self {
            text: Style::new(),
            dim: Style::new().add_modifier(Modifier::DIM),
            faint: Style::new().fg(Color::DarkGray),
            accent: Color::Yellow,
            playing: Color::Green,
            visual: Color::Blue,
            selection: Color::DarkGray,
            beat_row: None,
            tracks: vec![
                Color::Blue,
                Color::Green,
                Color::Magenta,
                Color::Cyan,
                Color::Yellow,
                Color::Red,
            ],
        }
    }

    pub fn track(&self, index: usize) -> Color {
        self.tracks[index % self.tracks.len()]
    }

    /// A solid block of `colour` with its text in the terminal's background colour. Uses reverse
    /// video rather than naming a text colour, so it stays readable with any palette.
    /// `Color::Reset` gives a block of the default foreground colour.
    pub fn block(&self, colour: Color) -> Style {
        Style::new()
            .fg(colour)
            .bg(Color::Reset)
            .add_modifier(Modifier::REVERSED | Modifier::BOLD)
            .remove_modifier(Modifier::DIM)
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::terminal()
    }
}
