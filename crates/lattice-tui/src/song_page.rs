//! Drawing the song page: the arrangement as a strip across the top, each entry as wide as it
//! plays for, then the song a bar to a row, time going down. On the left are each bar's time and
//! number, then the entries, each a block of rows in its pattern's colour, then a column for each
//! track showing how busy it is in each part of the bar. Under it all, the patterns there are to
//! choose from.

use lattice_core::{BEATS_PER_BAR, Lane};
use lattice_editor::Editor;
use lattice_engine::Status;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::theme::Theme;
use crate::ui::fit;

/// Width of the entries' column.
const ENTRY: usize = 16;
/// Width of the time and bar number on the left.
const GUTTER: usize = 12;
/// How busy part of a bar is, from a note on every step down to one now and then.
const BUSY: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

pub fn draw(frame: &mut Frame, area: Rect, editor: &Editor, status: &Status, theme: &Theme) {
    let arrangement = &editor.arrangement;
    let tracks = &editor.grid.tracks;
    let steps_per_beat = u64::from(editor.grid.steps_per_beat.max(1));
    let bar = u64::from(BEATS_PER_BAR) * steps_per_beat;
    let pattern_steps: Vec<u64> = (0..arrangement.patterns.len())
        .map(|p| arrangement.steps(&editor.grid, p))
        .collect();
    let entry_bars = editor.entry_bars();
    let total = entry_bars.iter().sum::<usize>() as u64 * bar;
    let time = |steps: u64| {
        let seconds = steps as f64 / steps_per_beat as f64 * 60.0 / editor.bpm;
        let seconds = seconds.round() as u64;
        format!("{}:{:02}", seconds / 60, seconds % 60)
    };
    let length = |steps: u64| {
        let bars = steps / bar;
        format!("{bars} bar{}", if bars == 1 { "" } else { "s" })
    };
    let playing = editor.song_position;
    let width = area.width as usize;
    // How far into the song the playhead is, while it plays from the arrangement.
    let playhead = playing
        .filter(|_| status.playing() && status.count_in() == 0)
        .and_then(|(entry, repeat)| {
            let steps = pattern_steps[arrangement.entries.get(entry)?.pattern];
            let start = editor.entry_start(entry) as u64 * bar;
            Some(start + u64::from(repeat) * steps + status.pattern_step().min(steps - 1))
        });
    let elapsed = playhead.map_or(String::new(), |at| format!("{} / ", time(at)));
    let lengths: Vec<u64> = entry_bars.iter().map(|&b| b as u64 * bar).collect();

    let mut lines = vec![
        Line::from(Span::styled(
            format!(
                " {} entries · {} · {elapsed}{}",
                arrangement.entries.len(),
                length(total),
                time(total)
            ),
            theme.dim,
        )),
        strip(editor, theme, &lengths, playhead, width.saturating_sub(2)),
        Line::raw(""),
    ];

    // Each track's column: a character for each beat, or each half beat where there's room,
    // scrolled sideways to keep the cursor's track in view.
    let room = width.saturating_sub(GUTTER + ENTRY);
    let beats = BEATS_PER_BAR as usize;
    let wide = steps_per_beat.is_multiple_of(2) && room / tracks.len().max(1) > beats * 2;
    let cells = if wide { beats * 2 } else { beats };
    let shown = (room / (cells + 2)).clamp(1, tracks.len().max(1));
    let column = editor.song_column();
    let first_track = column
        .saturating_sub(1)
        .saturating_sub(shown - 1)
        .min(tracks.len().saturating_sub(shown));
    let shown_tracks = first_track..(first_track + shown).min(tracks.len());

    let mut header = vec![
        Span::raw(" ".repeat(GUTTER)),
        Span::styled(fit("entries", ENTRY), theme.dim),
    ];
    for t in shown_tracks.clone() {
        header.push(Span::styled(
            fit(&tracks[t].name, cells + 2),
            Style::new().fg(theme.track(t)),
        ));
    }
    lines.push(Line::from(header));

    // The bars, scrolled to keep the cursor in view, leaving room for the patterns below.
    let all_bars: usize = entry_bars.iter().sum();
    let rows = (area.height as usize)
        .saturating_sub(lines.len() + 2)
        .max(1);
    let cursor = editor.song_bar();
    let first = cursor
        .saturating_sub(rows / 2)
        .min(all_bars.saturating_sub(rows));
    if arrangement.entries.is_empty() {
        lines.push(Line::styled(
            "   nothing to play yet: o adds an entry",
            theme.dim,
        ));
    }
    let playing_bar = playhead.map(|at| (at / bar) as usize);
    let mut lanes: Vec<Option<Vec<Lane>>> = vec![None; arrangement.patterns.len()];
    let mut start = 0;
    for (i, entry) in arrangement.entries.iter().enumerate() {
        let bars = entry_bars[i];
        let (from, to) = (start.max(first), (start + bars).min(first + rows));
        let pattern_bars = (bars / entry.repeats.max(1) as usize).max(1);
        let colour = theme.track(entry.pattern);
        let name = &arrangement.patterns[entry.pattern].name;
        let lanes = lanes[entry.pattern]
            .get_or_insert_with(|| arrangement.lanes(&editor.grid, entry.pattern));
        for at in from..to {
            let row = at - start;
            let repeat = row / pattern_bars;
            let in_pattern = row % pattern_bars;
            let here = at == cursor;

            // The time and bar number, the playhead marked.
            let is_playing = playing_bar == Some(at);
            let (marker, number_style) = if is_playing {
                ("▶", Style::new().fg(theme.playing))
            } else if here {
                (" ", Style::new().fg(theme.accent))
            } else {
                (" ", theme.dim)
            };
            let mut spans = vec![
                Span::styled(marker, Style::new().fg(theme.playing)),
                Span::styled(format!("{:>5} ", time(at as u64 * bar)), theme.dim),
                Span::styled(format!("{:>4}  ", at + 1), number_style),
            ];

            // The entry, as a bracket down its rows, named on its first.
            let edge = match (row, bars) {
                (_, 1) => '╶',
                (0, _) => '┌',
                (r, b) if r == b - 1 => '└',
                _ if in_pattern == 0 => '├',
                _ => '│',
            };
            // Named on its first row, or the first in view.
            let label = match row {
                _ if at == from && entry.repeats > 1 => format!("{name} ×{}", entry.repeats),
                _ if at == from => name.clone(),
                _ if in_pattern == 0 => format!("{}/{}", repeat + 1, entry.repeats),
                _ => String::new(),
            };
            let mut style = Style::new().fg(colour);
            if here && column == 0 {
                style = style.bg(theme.selection);
            }
            spans.push(Span::styled(
                format!("{edge} {}", fit(&label, ENTRY - 3)),
                style,
            ));
            spans.push(Span::raw(" "));

            // How busy each track is in this bar.
            for t in shown_tracks.clone() {
                let busy = lanes.get(t).map_or(String::new(), |lane| {
                    busy_bar(lane, in_pattern as u64 * bar, bar, cells)
                });
                let selected = here && column == t + 1;
                for c in busy.chars() {
                    let mut style = match c {
                        '·' | '─' => theme.faint,
                        _ => Style::new().fg(theme.track(t)),
                    };
                    if selected {
                        style = style.bg(theme.selection);
                    }
                    spans.push(Span::styled(c.to_string(), style));
                }
                spans.push(Span::raw("  "));
            }
            if is_playing {
                for span in &mut spans[1..] {
                    span.style = span.style.add_modifier(Modifier::BOLD);
                }
            }
            lines.push(Line::from(spans));
        }
        start += bars;
    }

    // The patterns to choose from, the one being edited underlined.
    let padding = (area.height as usize).saturating_sub(lines.len() + 1);
    lines.extend(std::iter::repeat_n(Line::raw(""), padding.min(1)));
    let mut patterns = vec![Span::styled(" patterns  ", theme.dim)];
    for (p, pattern) in arrangement.patterns.iter().enumerate() {
        let mut style = Style::new().fg(theme.track(p));
        if p == arrangement.current {
            style = style.add_modifier(Modifier::UNDERLINED);
        }
        patterns.push(Span::styled(pattern.name.clone(), style));
        patterns.push(Span::styled(
            format!(" {}   ", length(pattern_steps[p])),
            theme.dim,
        ));
    }
    lines.push(Line::from(patterns));
    frame.render_widget(Paragraph::new(lines), area);
}

/// How busy `lane` is in the bar starting at step `from`, `bar` steps long, in `cells` characters:
/// a block as tall as the share of steps with a note, a dot where there are none, and a line
/// across if the lane's muted. A lane shorter than the pattern plays round again.
fn busy_bar(lane: &Lane, from: u64, bar: u64, cells: usize) -> String {
    if lane.mute {
        return "─".repeat(cells);
    }
    let len = lane.steps.len() as u64;
    let per = bar / cells as u64;
    (0..cells as u64)
        .map(|c| {
            if len == 0 {
                return '·';
            }
            let notes = (0..per)
                .filter(|s| {
                    lane.steps[((from + c * per + s) % len) as usize]
                        .note
                        .is_some()
                })
                .count();
            match notes {
                0 => '·',
                n => BUSY[(n * BUSY.len()).div_ceil(per as usize).clamp(1, BUSY.len()) - 1],
            }
        })
        .collect()
}

/// The song in one line, each entry as wide as it plays for, coloured by its pattern and named
/// where there's room, with the playhead marked.
fn strip<'a>(
    editor: &'a Editor,
    theme: &Theme,
    lengths: &[u64],
    playhead: Option<u64>,
    width: usize,
) -> Line<'a> {
    let total: u64 = lengths.iter().sum();
    let mut cells: Vec<(char, Style)> = vec![];
    if total > 0 {
        let mut start = 0;
        for (i, entry) in editor.arrangement.entries.iter().enumerate() {
            start += lengths[i];
            // Rounded at the ends, so the strip fills the width exactly.
            let end = (start * width as u64).div_ceil(total) as usize;
            let room = end.saturating_sub(cells.len());
            if room == 0 {
                continue;
            }
            // A gap between entries, where there's room for one.
            let block = if room > 1 { room - 1 } else { 1 };
            let name = &editor.arrangement.patterns[entry.pattern].name;
            let style = theme.block(theme.track(entry.pattern));
            let text = format!(" {name}");
            let text = text.chars().chain(std::iter::repeat(' ')).take(block);
            cells.extend(text.map(|c| (c, style)));
            if room > 1 {
                cells.push((' ', Style::new()));
            }
        }
    }
    if let Some(at) = playhead {
        let cell = (at * width as u64 / total.max(1)) as usize;
        if let Some(c) = cells.get_mut(cell) {
            *c = (' ', theme.block(theme.playing));
        }
    }
    let mut spans = vec![Span::raw(" ")];
    spans.extend(
        cells
            .into_iter()
            .map(|(c, style)| Span::styled(c.to_string(), style)),
    );
    Line::from(spans)
}
