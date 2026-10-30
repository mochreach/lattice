//! Colours. The default follows the terminal's own palette; full themes can replace every value
//! later.

use ratatui::style::{Color, Modifier, Style};
use terminal_colorsaurus::QueryOptions;

/// How far beat rows move from the background towards the foreground.
const BEAT_ROW_SHADE: f32 = 0.07;

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

    /// The terminal theme, plus shading for beat rows mixed from the terminal's actual background
    /// and foreground colours. The palette alone has no subtle shade of the background. Must run
    /// before the terminal UI starts, as it reads the terminal's reply from stdin. If the terminal
    /// doesn't say what its colours are, beat rows stay unshaded.
    pub fn detect() -> Self {
        let mut theme = Self::terminal();
        if let Ok(palette) = terminal_colorsaurus::color_palette(QueryOptions::default()) {
            theme.beat_row = Some(mix(
                palette.background.scale_to_8bit(),
                palette.foreground.scale_to_8bit(),
                BEAT_ROW_SHADE,
            ));
        }
        theme
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

/// `amount` of the way from `from` to `to`.
fn mix(from: (u8, u8, u8), to: (u8, u8, u8), amount: f32) -> Color {
    let channel =
        |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * amount).round() as u8;
    Color::Rgb(
        channel(from.0, to.0),
        channel(from.1, to.1),
        channel(from.2, to.2),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_moves_towards_target() {
        assert_eq!(mix((0, 0, 0), (200, 100, 0), 0.0), Color::Rgb(0, 0, 0));
        assert_eq!(mix((0, 0, 0), (200, 100, 0), 0.5), Color::Rgb(100, 50, 0));
        // Light backgrounds darken.
        assert_eq!(
            mix((250, 250, 250), (0, 0, 0), 0.1),
            Color::Rgb(225, 225, 225)
        );
    }
}
