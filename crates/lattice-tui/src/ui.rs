//! Drawing the tracker.

use lattice_core::{Step, Track, realise};
use lattice_editor::{Editor, HELP, Mode, View};
use lattice_engine::Status;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

const ORANGE: Color = Color::Rgb(255, 106, 19);
const BLUE: Color = Color::Rgb(58, 134, 255);
const GREEN: Color = Color::Rgb(32, 191, 107);
const WHITE: Color = Color::Rgb(228, 228, 228);
const PINK: Color = Color::Rgb(255, 77, 109);
const PURPLE: Color = Color::Rgb(170, 120, 255);
const TEXT: Color = Color::Rgb(200, 200, 200);
const DIM: Color = Color::Rgb(110, 110, 110);
const FAINT: Color = Color::Rgb(60, 60, 60);
const BEAT_ROW: Color = Color::Rgb(26, 26, 26);
const SELECTED: Color = Color::Rgb(72, 72, 72);

/// Each track gets its own colour, like the OP-1's four knobs.
const TRACK_COLOURS: [Color; 6] = [BLUE, GREEN, WHITE, ORANGE, PINK, PURPLE];

/// Width of the step-number column.
const GUTTER: usize = 4;
/// Width of a track's cells, not counting the gap after them.
const CELL: usize = 11;
const BEATS_PER_BAR: usize = 4;

#[derive(Debug, Default)]
pub struct ViewState {
    scroll: usize,
}

pub fn draw(frame: &mut Frame, editor: &Editor, status: &Status, view: &mut ViewState) {
    let [top, grid, bottom] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Fill(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    frame.render_widget(Paragraph::new(header(editor, status)), top);
    draw_grid(frame, grid, editor, status, view);
    frame.render_widget(Paragraph::new(status_line(editor, bottom.width)), bottom);
}

fn header(editor: &Editor, status: &Status) -> Line<'static> {
    let label = |s: &'static str| Span::styled(s, Style::new().fg(DIM));
    let value = |s: String| Span::styled(s, Style::new().fg(TEXT).add_modifier(Modifier::BOLD));
    let on_off = |on: bool| if on { "on" } else { "off" };
    let transport = if status.playing() {
        Span::styled(
            " ▶ PLAY ",
            Style::new()
                .fg(Color::Black)
                .bg(GREEN)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(" ■ STOP ", Style::new().fg(TEXT).bg(FAINT))
    };
    let view = match editor.view {
        View::Source => "written",
        View::Live => "live",
    };
    Line::from(vec![
        Span::styled(
            " LATTICE ",
            Style::new()
                .fg(Color::Black)
                .bg(ORANGE)
                .add_modifier(Modifier::BOLD),
        ),
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
    fn new(index: usize, track: &'a Track, editor: &Editor, status: &Status) -> Self {
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
            track,
            shown,
            tails,
            playhead: position.map(|(step, _)| step),
            pass: position.map(|(_, pass)| pass),
            colour: TRACK_COLOURS[index % TRACK_COLOURS.len()],
        }
    }
}

fn draw_grid(
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    status: &Status,
    view: &mut ViewState,
) {
    let columns: Vec<Column> = editor
        .grid
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| Column::new(i, t, editor, status))
        .collect();
    let mut lines = header_lines(&columns);
    let rows = (area.height as usize).saturating_sub(lines.len());
    let longest = columns.iter().map(|c| c.shown.len()).max().unwrap_or(0);
    scroll_to(view, editor, &columns, rows, longest, status.playing());

    let steps_per_beat = editor.grid.steps_per_beat.max(1) as usize;
    for row in (view.scroll..longest).take(rows) {
        lines.push(grid_row(row, steps_per_beat, &columns, editor));
    }
    frame.render_widget(Paragraph::new(lines), area);
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
    let followed = columns[editor.cursor.track]
        .playhead
        .filter(|_| editor.follow && playing);
    let target = followed.unwrap_or(editor.cursor.step);
    if target < view.scroll {
        view.scroll = target;
    } else if rows > 0 && target >= view.scroll + rows {
        view.scroll = target + 1 - rows;
    }
    view.scroll = view.scroll.min(longest.saturating_sub(rows));
}

/// Track names, lengths and passes, then the rules one per line, then an underline.
fn header_lines(columns: &[Column]) -> Vec<Line<'static>> {
    let cell = |text: String, style: Style| Span::styled(format!("{} ", fit(&text, CELL)), style);
    let line = |cells: Vec<Span<'static>>| {
        let mut spans = vec![Span::raw(" ".repeat(GUTTER))];
        spans.extend(cells);
        Line::from(spans)
    };

    let mut lines = vec![
        line(
            columns
                .iter()
                .map(|c| {
                    cell(
                        c.track.name.to_uppercase(),
                        Style::new().fg(c.colour).add_modifier(Modifier::BOLD),
                    )
                })
                .collect(),
        ),
        line(
            columns
                .iter()
                .map(|c| {
                    let pass = c.pass.map(|p| format!(" · pass {p}")).unwrap_or_default();
                    cell(
                        format!("{}{pass}", c.track.steps.len()),
                        Style::new().fg(DIM),
                    )
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
                    None if k == 0 => cell("no rules".into(), Style::new().fg(FAINT)),
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
) -> Line<'static> {
    let on_bar = row.is_multiple_of(steps_per_beat * BEATS_PER_BAR);
    let on_beat = row.is_multiple_of(steps_per_beat);
    let gutter_style = if on_bar {
        Style::new().fg(TEXT).add_modifier(Modifier::BOLD)
    } else if on_beat {
        Style::new().fg(DIM)
    } else {
        Style::new().fg(FAINT)
    };
    let mut spans = vec![Span::styled(format!(" {row:02X} "), gutter_style)];

    let (sel_start, sel_end) = editor.selection();
    for (i, c) in columns.iter().enumerate() {
        let Some(step) = c.shown.get(row) else {
            spans.push(Span::raw(" ".repeat(CELL + 1)));
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
            None => (" ···".into(), Style::new().fg(FAINT)),
        };
        // In the live view, highlight what the rules have changed.
        if editor.view == View::Live && c.track.steps.get(row) != Some(step) {
            style = style.add_modifier(Modifier::BOLD | Modifier::ITALIC);
        }
        if on_beat {
            style = style.bg(BEAT_ROW);
        }

        let is_cursor = editor.cursor.track == i && editor.cursor.step == row;
        let selected = matches!(editor.mode, Mode::Visual { .. })
            && editor.cursor.track == i
            && (sel_start..=sel_end).contains(&row);
        if is_cursor {
            let bg = match editor.mode {
                Mode::Insert => ORANGE,
                Mode::Visual { .. } => BLUE,
                _ => TEXT,
            };
            style = style.fg(Color::Black).bg(bg).remove_modifier(Modifier::DIM);
        } else if c.playhead == Some(row) {
            style = style
                .fg(Color::Black)
                .bg(c.colour)
                .add_modifier(Modifier::BOLD)
                .remove_modifier(Modifier::DIM);
        } else if selected {
            style = style.bg(SELECTED);
        }

        spans.push(Span::styled(fit(&text, CELL), style));
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
}

fn status_line(editor: &Editor, width: u16) -> Line<'static> {
    let badge = |text: &'static str, bg: Color| {
        Span::styled(
            text,
            Style::new()
                .fg(Color::Black)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )
    };
    let mut left = match &editor.mode {
        Mode::Normal => vec![badge(" NORMAL ", DIM)],
        Mode::Insert => vec![badge(" INSERT ", ORANGE)],
        Mode::Visual { .. } => vec![badge(" VISUAL ", BLUE)],
        Mode::Command(line) => vec![
            Span::styled(format!(":{line}"), Style::new().fg(TEXT)),
            Span::styled(" ", Style::new().bg(TEXT)),
        ],
    };
    if let Some(message) = &editor.message {
        left.push(Span::styled(format!(" {message}"), Style::new().fg(ORANGE)));
    } else if let Some(count) = editor.count() {
        left.push(Span::styled(format!(" {count}"), Style::new().fg(TEXT)));
    }

    let hints = match editor.mode {
        Mode::Normal => "i insert · v select · : command · space play · tab live · u undo",
        Mode::Insert => "z–m q–u notes · [ ] octave · ⌫ clear · esc done",
        Mode::Visual { .. } => "y copy · d clear · ^a ^x transpose · < > octave · + - length",
        Mode::Command(_) => HELP,
    };
    let used: usize = left.iter().map(|s| s.content.chars().count()).sum();
    let room = (width as usize).saturating_sub(used + 1);
    let hints: String = hints.chars().take(room).collect();
    left.push(Span::raw(
        " ".repeat(room - hints.chars().count().min(room)),
    ));
    left.push(Span::styled(hints, Style::new().fg(DIM)));
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
    fn fit_pads_and_truncates() {
        assert_eq!(fit("ab", 4), "ab  ");
        assert_eq!(fit("abcdef", 4), "abcd");
        assert_eq!(fit("—·", 3), "—· ");
    }
}
