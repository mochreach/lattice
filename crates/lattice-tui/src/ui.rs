//! Drawing the tracker.

use lattice_core::{Step, Track, realise};
use lattice_editor::{BEATS_PER_BAR, Completion, Editor, Infobox, Mode, View};
use lattice_engine::Status;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Clear, Padding, Paragraph};

use crate::theme::Theme;

/// Width of the step-number column.
const GUTTER: usize = 4;
/// Width of a track's cells, not counting the gap after them.
const CELL: usize = 11;

#[derive(Debug, Default)]
pub struct ViewState {
    scroll: usize,
    /// The leftmost track shown, when they don't all fit.
    first_track: usize,
}

pub fn draw(
    frame: &mut Frame,
    editor: &Editor,
    status: &Status,
    theme: &Theme,
    view: &mut ViewState,
) {
    let [top, grid, bottom] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    frame.render_widget(Paragraph::new(header(editor, status, theme)), top);
    draw_grid(frame, grid, editor, status, theme, view);
    frame.render_widget(
        Paragraph::new(status_line(editor, theme, bottom.width)),
        bottom,
    );
    if let Some(infobox) = editor.infobox() {
        draw_infobox(frame, grid, &infobox, theme);
    }
    if let Mode::Command(prompt) = &editor.mode {
        let completions = editor.completions();
        if !completions.is_empty() {
            draw_completions(frame, grid, &completions, prompt.selected, theme);
        }
    }
}

/// Most completions shown at once.
const COMPLETIONS: usize = 10;

/// Commands matching what's typed, above the command line, with the matched letters picked out.
fn draw_completions(
    frame: &mut Frame,
    area: Rect,
    completions: &[Completion],
    selected: Option<usize>,
    theme: &Theme,
) {
    let name_width = completions
        .iter()
        .map(|c| c.spec.name.len())
        .max()
        .unwrap_or(0);
    let signature_width = completions
        .iter()
        .map(|c| c.spec.signature().chars().count())
        .max()
        .unwrap_or(0);
    // Keep the selected one in view while cycling.
    let first = selected.map_or(0, |s| (s + 1).saturating_sub(COMPLETIONS));
    let lines: Vec<Line> = completions
        .iter()
        .enumerate()
        .skip(first)
        .take(COMPLETIONS)
        .map(|(i, c)| {
            let mut spans: Vec<Span> = c
                .spec
                .name
                .char_indices()
                .map(|(j, ch)| {
                    let style = if c.matched.contains(&j) {
                        Style::new().fg(theme.accent).add_modifier(Modifier::BOLD)
                    } else {
                        theme.text
                    };
                    Span::styled(ch.to_string(), style)
                })
                .collect();
            spans.push(Span::raw(" ".repeat(name_width - c.spec.name.len() + 2)));
            spans.push(Span::styled(
                format!("{:<signature_width$}  ", c.spec.signature()),
                theme.faint,
            ));
            spans.push(Span::styled(c.spec.describe, theme.dim));
            let line = Line::from(spans);
            if selected == Some(i) {
                line.style(Style::new().bg(theme.selection))
            } else {
                line
            }
        })
        .collect();
    let width = (lines.iter().map(Line::width).max().unwrap_or(0) as u16 + 4).min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    let corner = Rect {
        x: area.x,
        y: area.bottom().saturating_sub(height),
        width,
        height,
    };
    frame.render_widget(Clear, corner);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .border_style(theme.dim)
                .padding(Padding::horizontal(1)),
        ),
        corner,
    );
}

/// Like Helix's: the keys that can follow a prefix key, in the bottom-right corner above the
/// status line.
fn draw_infobox(frame: &mut Frame, area: Rect, infobox: &Infobox, theme: &Theme) {
    let key_width = infobox
        .items
        .iter()
        .map(|(key, _)| key.chars().count())
        .max()
        .unwrap_or(0);
    let lines: Vec<Line> = infobox
        .items
        .iter()
        .map(|(key, description)| {
            Line::from(vec![
                Span::styled(
                    format!("{key:<key_width$}"),
                    Style::new().fg(theme.accent).add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  {description}"), theme.text),
            ])
        })
        .collect();
    let content = lines.iter().map(Line::width).max().unwrap_or(0);
    let width = (content.max(infobox.title.len()) as u16 + 4).min(area.width);
    let height = (lines.len() as u16 + 2).min(area.height);
    let corner = Rect {
        x: area.right().saturating_sub(width + 1),
        y: area.bottom().saturating_sub(height),
        width,
        height,
    };
    frame.render_widget(Clear, corner);
    frame.render_widget(
        Paragraph::new(lines).block(
            Block::bordered()
                .title(format!(" {} ", infobox.title))
                .border_style(theme.dim)
                .padding(Padding::horizontal(1)),
        ),
        corner,
    );
}

fn header(editor: &Editor, status: &Status, theme: &Theme) -> Line<'static> {
    let label = |s: &'static str| Span::styled(s, theme.dim);
    let value = |s: String| Span::styled(s, theme.text.add_modifier(Modifier::BOLD));
    let on_off = |on: bool| if on { "on" } else { "off" };
    let transport = if status.playing() {
        Span::styled(" ▶ PLAY ", theme.block(theme.playing))
    } else {
        Span::styled(" ■ STOP ", theme.block(Color::DarkGray))
    };
    let view = match editor.view {
        View::Source => "written",
        View::Live => "live",
    };
    Line::from(vec![
        Span::styled(" LATTICE ", theme.block(theme.accent)),
        Span::raw(" "),
        transport,
        label("   bpm "),
        value(format!("{:.0}", editor.bpm)),
        label("   oct "),
        value(editor.octave.to_string()),
        label("   step "),
        value(editor.auto_step.to_string()),
        label("   view "),
        value(view.into()),
        label("   follow "),
        value(on_off(editor.follow).into()),
        label("   click "),
        value(on_off(editor.click).into()),
    ])
}

/// What one track column shows this frame.
struct Column<'a> {
    index: usize,
    track: &'a Track,
    /// Written or realised, depending on the view.
    shown: Vec<Step>,
    /// Rows covered by an earlier note's length.
    tails: Vec<bool>,
    playhead: Option<usize>,
    pass: Option<u32>,
    colour: Color,
}

impl<'a> Column<'a> {
    fn new(
        index: usize,
        track: &'a Track,
        editor: &Editor,
        status: &Status,
        theme: &Theme,
    ) -> Self {
        let position = status.position(index);
        let shown = match editor.view {
            View::Source => track.steps.clone(),
            View::Live => realise(track, position.map_or(0, |(_, pass)| pass)),
        };
        let mut tails = vec![false; shown.len()];
        for (i, step) in shown.iter().enumerate() {
            if let Some(note) = step.note {
                for tail in tails
                    .iter_mut()
                    .skip(i + 1)
                    .take(usize::from(note.length) - 1)
                {
                    *tail = true;
                }
            }
        }
        Self {
            index,
            track,
            shown,
            tails,
            playhead: position.map(|(step, _)| step),
            pass: position.map(|(_, pass)| pass),
            colour: theme.track(index),
        }
    }
}

fn draw_grid(
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    status: &Status,
    theme: &Theme,
    view: &mut ViewState,
) {
    let tracks = editor.grid.tracks.len();
    // One column spare for the arrow that shows there are more tracks.
    let fits = ((area.width as usize).saturating_sub(GUTTER + 1) / (CELL + 1)).max(1);
    scroll_tracks(view, editor.cursor.track, fits, tracks);
    let columns: Vec<Column> = editor
        .grid
        .tracks
        .iter()
        .enumerate()
        .skip(view.first_track)
        .take(fits)
        .map(|(i, t)| Column::new(i, t, editor, status, theme))
        .collect();
    let hidden = (view.first_track > 0, view.first_track + fits < tracks);
    let mut lines = header_lines(&columns, hidden, theme);
    let rows = (area.height as usize).saturating_sub(lines.len());
    let longest = columns.iter().map(|c| c.shown.len()).max().unwrap_or(0);
    scroll_to(view, editor, &columns, rows, longest, status.playing());

    let steps_per_beat = editor.grid.steps_per_beat.max(1) as usize;
    for row in (view.scroll..longest).take(rows) {
        lines.push(grid_row(row, steps_per_beat, &columns, editor, theme));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Keeps the cursor's track on screen when there are more tracks than fit.
fn scroll_tracks(view: &mut ViewState, cursor: usize, fits: usize, tracks: usize) {
    if cursor < view.first_track {
        view.first_track = cursor;
    } else if cursor >= view.first_track + fits {
        view.first_track = cursor + 1 - fits;
    }
    view.first_track = view.first_track.min(tracks.saturating_sub(fits));
}

/// Keeps the cursor on screen, or the playhead when following playback.
fn scroll_to(
    view: &mut ViewState,
    editor: &Editor,
    columns: &[Column],
    rows: usize,
    longest: usize,
    playing: bool,
) {
    let followed = columns
        .iter()
        .find(|c| c.index == editor.cursor.track)
        .and_then(|c| c.playhead)
        .filter(|_| editor.follow && playing);
    let target = followed.unwrap_or(editor.cursor.step);
    if target < view.scroll {
        view.scroll = target;
    } else if rows > 0 && target >= view.scroll + rows {
        view.scroll = target + 1 - rows;
    }
    view.scroll = view.scroll.min(longest.saturating_sub(rows));
}

/// Track names, lengths and passes, then the rules one per line, then an underline. Arrows by
/// the names show there are more tracks off to the (left, right).
fn header_lines(
    columns: &[Column],
    (left, right): (bool, bool),
    theme: &Theme,
) -> Vec<Line<'static>> {
    let cell = |text: String, style: Style| Span::styled(format!("{} ", fit(&text, CELL)), style);
    let line = |cells: Vec<Span<'static>>| {
        let mut spans = vec![Span::raw(" ".repeat(GUTTER))];
        spans.extend(cells);
        Line::from(spans)
    };

    let mut names = line(
        columns
            .iter()
            .map(|c| {
                cell(
                    c.track.name.to_uppercase(),
                    Style::new().fg(c.colour).add_modifier(Modifier::BOLD),
                )
            })
            .collect(),
    );
    if left {
        names.spans[0] = Span::styled(fit(" ‹", GUTTER), theme.dim);
    }
    if right {
        names.spans.push(Span::styled("›", theme.dim));
    }
    let mut lines = vec![
        names,
        line(
            columns
                .iter()
                .map(|c| {
                    let pass = c.pass.map(|p| format!(" · pass {p}")).unwrap_or_default();
                    cell(format!("{}{pass}", c.track.steps.len()), theme.dim)
                })
                .collect(),
        ),
    ];
    let rule_rows = columns
        .iter()
        .map(|c| c.track.rules.len())
        .max()
        .unwrap_or(0)
        .max(1);
    for k in 0..rule_rows {
        lines.push(line(
            columns
                .iter()
                .map(|c| match c.track.rules.get(k) {
                    Some(rule) => cell(rule.to_string(), Style::new().fg(c.colour)),
                    None if k == 0 => cell("no rules".into(), theme.faint),
                    None => cell(String::new(), Style::new()),
                })
                .collect(),
        ));
    }
    lines.push(line(
        columns
            .iter()
            .map(|c| cell("─".repeat(CELL), Style::new().fg(c.colour)))
            .collect(),
    ));
    lines
}

fn grid_row(
    row: usize,
    steps_per_beat: usize,
    columns: &[Column],
    editor: &Editor,
    theme: &Theme,
) -> Line<'static> {
    let on_bar = row.is_multiple_of(steps_per_beat * BEATS_PER_BAR);
    let on_beat = row.is_multiple_of(steps_per_beat);
    let gutter_style = if on_bar {
        theme.text.add_modifier(Modifier::BOLD)
    } else if on_beat {
        theme.text
    } else {
        theme.faint
    };
    // Shade the whole row on the beat, gutter and gaps included, so it reads as one band.
    let band = match theme.beat_row {
        Some(bg) if on_beat => Style::new().bg(bg),
        _ => Style::new(),
    };
    let mut spans = vec![Span::styled(
        format!(" {row:02X} "),
        gutter_style.patch(band),
    )];

    let selection = editor.selection();
    for c in columns {
        let i = c.index;
        let Some(step) = c.shown.get(row) else {
            spans.push(Span::styled(" ".repeat(CELL + 1), band));
            continue;
        };

        let (text, mut style) = match step.note {
            Some(note) => {
                let length = if note.length > 1 {
                    note.length.to_string()
                } else {
                    String::new()
                };
                (
                    format!(" {} {length:>4}  ", note_name(note.pitch)),
                    Style::new().fg(c.colour),
                )
            }
            None if c.tails[row] => (
                " │".into(),
                Style::new().fg(c.colour).add_modifier(Modifier::DIM),
            ),
            None => (" ···".into(), theme.faint),
        };
        // In the live view, highlight what the rules have changed.
        if editor.view == View::Live && c.track.steps.get(row) != Some(step) {
            style = style.add_modifier(Modifier::BOLD | Modifier::ITALIC);
        }
        style = style.patch(band);

        let is_cursor = editor.cursor.track == i && editor.cursor.step == row;
        let selected = selection.contains(i, row);
        if is_cursor {
            style = theme.block(match editor.mode {
                Mode::Insert => theme.accent,
                Mode::Select => theme.visual,
                _ => Color::Reset,
            });
        } else if c.playhead == Some(row) {
            style = theme.block(c.colour);
        } else if selected {
            style = style.bg(theme.selection);
        }

        spans.push(Span::styled(fit(&text, CELL), style));
        spans.push(Span::styled(" ", band));
    }
    Line::from(spans)
}

fn status_line(editor: &Editor, theme: &Theme, width: u16) -> Line<'static> {
    let mut left = match &editor.mode {
        Mode::Normal => vec![Span::styled(" NORMAL ", theme.block(Color::DarkGray))],
        Mode::Insert => vec![Span::styled(" INSERT ", theme.block(theme.accent))],
        Mode::Select => vec![Span::styled(" SELECT ", theme.block(theme.visual))],
        Mode::Command(prompt) => {
            // The parameters still to type sit faintly after the cursor, to be typed over.
            let hint = editor.command_hint().unwrap_or_default();
            let mut hint = hint.chars();
            let under_cursor = hint.next().unwrap_or(' ');
            vec![
                Span::styled(format!(":{}", prompt.line), theme.text),
                Span::styled(
                    under_cursor.to_string(),
                    theme.faint.add_modifier(Modifier::REVERSED),
                ),
                Span::styled(hint.collect::<String>(), theme.faint),
            ]
        }
    };
    if let Some(message) = &editor.message {
        left.push(Span::styled(
            format!(" {message}"),
            Style::new().fg(theme.accent),
        ));
    } else if let Some(count) = editor.count() {
        left.push(Span::styled(format!(" {count}"), theme.text));
    }

    let hints = match &editor.mode {
        Mode::Normal => "v select · x rows · g goto · ] next · i insert · d delete · : command",
        Mode::Insert => "z–m q–u notes · [ ] octave · ⌫ clear · esc done",
        Mode::Select => "hjkl extend · ; collapse · y copy · d delete · r fill · C-a C-x transpose",
        Mode::Command(_) => editor
            .command_spec()
            .map_or("tab complete · ret run · esc cancel", |spec| {
                spec.describe
            }),
    };
    let used: usize = left.iter().map(|s| s.content.chars().count()).sum();
    let room = (width as usize).saturating_sub(used + 1);
    let hints: String = hints.chars().take(room).collect();
    left.push(Span::raw(
        " ".repeat(room - hints.chars().count().min(room)),
    ));
    left.push(Span::styled(hints, theme.dim));
    Line::from(left)
}

/// Pads or truncates to exactly `width` characters.
fn fit(text: &str, width: usize) -> String {
    let mut s: String = text.chars().take(width).collect();
    let len = s.chars().count();
    s.extend(std::iter::repeat_n(' ', width - len));
    s
}

/// Tracker-style note names: `C-4`, `F#2`.
fn note_name(pitch: u8) -> String {
    const NAMES: [&str; 12] = [
        "C-", "C#", "D-", "D#", "E-", "F-", "F#", "G-", "G#", "A-", "A#", "B-",
    ];
    let octave = i32::from(pitch / 12) - 1;
    let octave = if octave < 0 {
        "-".to_string()
    } else {
        octave.to_string()
    };
    format!("{}{octave}", NAMES[usize::from(pitch % 12)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_names() {
        assert_eq!(note_name(60), "C-4");
        assert_eq!(note_name(70), "A#4");
        assert_eq!(note_name(1), "C#-");
        assert_eq!(note_name(127), "G-9");
    }

    #[test]
    fn tracks_scroll_to_keep_the_cursor_in_view() {
        let mut view = ViewState::default();
        scroll_tracks(&mut view, 5, 3, 8);
        assert_eq!(view.first_track, 3);
        scroll_tracks(&mut view, 4, 3, 8);
        assert_eq!(view.first_track, 3, "still in view");
        scroll_tracks(&mut view, 1, 3, 8);
        assert_eq!(view.first_track, 1);
        scroll_tracks(&mut view, 1, 3, 2);
        assert_eq!(view.first_track, 0, "everything fits again");
    }

    #[test]
    fn fit_pads_and_truncates() {
        assert_eq!(fit("ab", 4), "ab  ");
        assert_eq!(fit("abcdef", 4), "abcd");
        assert_eq!(fit("—·", 3), "—· ");
    }
}
