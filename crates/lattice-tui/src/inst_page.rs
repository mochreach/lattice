//! Drawing the instrument page: every track's instrument side by side, a column each read top to
//! bottom, like the fx page's lanes. Each column has the instrument's name, its settings a line
//! each with a bar, the track's macros, and its chain of effects. A track with a face shows its
//! title, its macros and the settings it puts forward instead, under its art (see [`face`]).

use std::time::{Duration, Instant};

use lattice_core::macros::{self, MACRO_INFO};
use lattice_core::{Kind, Locks, Track};
use lattice_editor::{Editor, InstLine, Strip};
use lattice_engine::Status;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::bars;
use crate::face::{self, Feel};
use crate::mixer_page::flag;
use crate::theme::Theme;
use crate::ui::{ViewState, fit};

/// Width of the line numbers down the left.
const LABEL: usize = 5;
/// Narrowest and widest a column gets.
const MIN_COLUMN: usize = 24;
const MAX_COLUMN: usize = 34;
/// Width of a setting's name.
const PARAM_LABEL: usize = 11;
/// Lines above the columns (the rule, a gap and the tracks' names).
const ABOVE: usize = 3;
/// Most and fewest rows of art, and the lines left for the columns below it at least.
const MAX_ART: usize = 8;
const MIN_ART: usize = 3;
const BELOW_ART: usize = 6;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    status: &Status,
    theme: &Theme,
    view: &mut ViewState,
) {
    let width = area.width as usize;
    let cursor = editor.inst_cursor();
    let tracks = editor.grid.tracks.len();
    // A song always has a track, but with none there'd be nothing to draw.
    if tracks == 0 {
        return;
    }

    // As many columns as fit, scrolled to keep the cursor's in view.
    let shown = (width.saturating_sub(LABEL) / MIN_COLUMN).clamp(1, tracks);
    if cursor.track < view.inst_left {
        view.inst_left = cursor.track;
    } else if cursor.track >= view.inst_left + shown {
        view.inst_left = cursor.track + 1 - shown;
    }
    view.inst_left = view.inst_left.min(tracks.saturating_sub(shown));
    let columns: Vec<(usize, &Track)> = (editor.grid.tracks.iter().enumerate())
        .skip(view.inst_left)
        .take(shown)
        .collect();
    let column_width = (width.saturating_sub(LABEL) / shown).clamp(MIN_COLUMN, MAX_COLUMN);
    let all: Vec<Vec<InstLine>> = columns.iter().map(|&(t, _)| editor.inst_lines(t)).collect();
    let longest = all.iter().map(Vec::len).max().unwrap_or(0);

    let left = if view.inst_left > 0 { "‹" } else { "─" };
    let right = if view.inst_left + shown < tracks {
        " ›"
    } else {
        ""
    };
    let title = format!("─{left} instruments{right} ");
    let mut lines = vec![
        Line::from(vec![
            Span::raw(" "),
            Span::styled(title.clone(), theme.text.add_modifier(Modifier::BOLD)),
            Span::styled(
                "─".repeat(width.saturating_sub(title.chars().count() + 1)),
                theme.faint,
            ),
        ]),
        Line::raw(""),
    ];

    // Each track's name, in its colour, with its mute or solo.
    let mut spans = vec![Span::raw(" ".repeat(LABEL))];
    for &(t, track) in &columns {
        let (flag, flag_colour) = flag(editor, Strip::Track(t), theme);
        let style = Style::new().fg(theme.track(t)).add_modifier(Modifier::BOLD);
        let name = fit(&format!(" {}", track.name.to_uppercase()), column_width - 3);
        spans.push(Span::styled(name, style));
        spans.push(Span::styled(
            format!("{flag:<3}"),
            Style::new().fg(flag_colour).add_modifier(Modifier::BOLD),
        ));
    }
    lines.push(Line::from(spans));

    // The faces' art, under their names, as tall as leaves room for the lines.
    let height = (area.height as usize).saturating_sub(ABOVE);
    let faces = columns.iter().any(|&(t, _)| editor.shows_face(t));
    let art = match (height.saturating_sub(BELOW_ART) / 2).min(MAX_ART) {
        rows if faces && rows >= MIN_ART => rows,
        _ => 0,
    };
    lines.extend((0..art).map(|_| Line::raw("")));

    // As many lines as fit, scrolled to keep the cursor's in view.
    let room = height.saturating_sub(art).max(1);
    if cursor.line < view.inst_top {
        view.inst_top = cursor.line;
    } else if cursor.line >= view.inst_top + room {
        view.inst_top = cursor.line + 1 - room;
    }
    view.inst_top = view.inst_top.min(longest.saturating_sub(room));

    for l in (view.inst_top..longest).take(room) {
        let number = if cursor.line == l {
            theme.text
        } else {
            theme.faint
        };
        let mut spans = vec![Span::styled(fit(&format!(" {l:02}"), LABEL), number)];
        for (&(t, track), lines) in columns.iter().zip(&all) {
            let here = cursor.track == t && cursor.line == l;
            let cell = match lines.get(l) {
                Some(&line) => cell(track, status, theme, t, line, here, column_width),
                None => vec![],
            };
            let used: usize = cell.iter().map(|s| s.content.chars().count()).sum();
            spans.extend(cell);
            spans.push(Span::raw(" ".repeat(column_width.saturating_sub(used))));
        }
        lines.push(Line::from(spans));
    }
    frame.render_widget(Paragraph::new(lines), area);

    if art == 0 {
        return;
    }
    let now = Instant::now();
    let time = view.face_clock.seconds();
    for (k, &(t, track)) in columns.iter().enumerate() {
        let Some(face) = track.face.as_ref().filter(|_| editor.shows_face(t)) else {
            continue;
        };
        // A new note's when the count of them changes.
        let note = status.last_note(t);
        // There's a slot for every track the engine can have.
        let Some(seen) = view.notes.get_mut(t) else {
            continue;
        };
        if let Some((_, _, count)) = note
            && seen.is_none_or(|(c, _)| c != count)
        {
            // Not on first sight, only for the notes after.
            let when = if seen.is_some() {
                now
            } else {
                now - Duration::from_secs(1)
            };
            *seen = Some((count, when));
        }
        let since_note = seen.map_or(f32::MAX, |(_, when)| (now - when).as_secs_f32());
        let locks = status.locks(t);
        let macro_value = |m: usize| {
            let lock = locks
                .iter()
                .find(|l| l.is_macro() && usize::from(l.param) == m);
            lock.map(|l| l.value)
                .or_else(|| track.macros.get(m).map(|m| m.value))
                .unwrap_or(0.0)
        };
        let feel = Feel {
            level: view.meters.get(t).copied().unwrap_or(0.0),
            note: note.map(|(pitch, velocity, _)| (pitch, velocity)),
            since_note,
            macros: [macro_value(0), macro_value(1)],
        };
        let x = area.x + (LABEL + k * column_width) as u16 + 1;
        let rect = Rect::new(
            x,
            area.y + ABOVE as u16,
            column_width as u16 - 3,
            art as u16,
        )
        .intersection(area);
        face::draw(frame.buffer_mut(), rect, &face.title, &feel, theme, time);
    }
}

/// Track `t`'s `line`, a column wide less a space.
fn cell(
    track: &Track,
    status: &Status,
    theme: &Theme,
    t: usize,
    line: InstLine,
    here: bool,
    width: usize,
) -> Vec<Span<'static>> {
    let colour = theme.track(t);
    let w = width - 1;
    let name_style = if here {
        theme.text.add_modifier(Modifier::BOLD)
    } else {
        theme.dim
    };
    // The value under the cursor in the terminal's own colour, as on the mixer.
    let value_style = if here {
        Style::new().fg(Color::Reset)
    } else {
        Style::new().fg(colour)
    };
    let block = |style: Style| {
        if here {
            theme.block(Color::Reset)
        } else {
            style
        }
    };
    // While it plays, a setting or macro a step has locked shows the lock's value.
    let locks = if status.playing() {
        status.locks(t)
    } else {
        Locks::default()
    };
    let locked = Style::new().fg(theme.accent).add_modifier(Modifier::BOLD);
    match line {
        InstLine::Kind => {
            let kind = track.instrument.kind.name();
            // A face's title, and faintly what it's played on.
            let (name, under) = match &track.face {
                Some(face) => {
                    let title = fit(&face.title, w.saturating_sub(kind.len() + 4));
                    (title.trim_end().to_string(), format!(" {kind}"))
                }
                None => (kind.to_string(), String::new()),
            };
            let used = name.chars().count() + under.chars().count() + 2;
            let rule = "─".repeat(w.saturating_sub(used));
            vec![
                Span::styled(
                    format!(" {name}"),
                    block(Style::new().fg(colour).add_modifier(Modifier::BOLD)),
                ),
                Span::styled(under, theme.faint),
                Span::styled(format!(" {rule}"), Style::new().fg(colour)),
            ]
        }
        InstLine::Param(p) if let Some(info) = track.instrument.params().get(p) => {
            let lock = (locks.iter()).find(|l| l.fx.is_none() && u32::from(l.param) == info.id);
            // A macro turning it is marked with its number.
            let mark = macros::mark(&track.macros, None, info.id as u8);
            let (mark, value, label_style, style) = match (lock, mark) {
                (Some(lock), _) => (Span::styled(" ◆", locked), lock.value, locked, locked),
                (None, Some(m)) => (
                    Span::styled(format!(" {m}"), Style::new().fg(theme.visual)),
                    track.instrument.get(info.id),
                    name_style,
                    value_style,
                ),
                (None, None) => (
                    Span::raw("  "),
                    track.instrument.get(info.id),
                    name_style,
                    value_style,
                ),
            };
            let mut spans = vec![
                mark,
                Span::styled(fit(info.name, PARAM_LABEL - 2), label_style),
            ];
            spans.extend(bars::param(info, value, w - PARAM_LABEL, style, theme));
            spans
        }
        InstLine::Macro(m) if let Some(mac) = track.macros.get(m) => {
            let lock = (locks.iter()).find(|l| l.is_macro() && usize::from(l.param) == m);
            let (value, label_style, style) = match lock {
                Some(lock) => (lock.value, locked, locked),
                None => (
                    mac.value,
                    block(Style::new().fg(theme.visual).add_modifier(Modifier::BOLD)),
                    value_style,
                ),
            };
            let label = format!(" {} {}", m + 1, mac.name);
            let mut spans = vec![Span::styled(fit(&label, PARAM_LABEL), label_style)];
            spans.extend(bars::param(
                &MACRO_INFO,
                value,
                w - PARAM_LABEL,
                style,
                theme,
            ));
            spans
        }
        // A setting or macro that's gone draws as nothing, until the editor's lines catch up.
        InstLine::Param(_) | InstLine::Macro(_) => vec![],
        InstLine::Chain => {
            let names: Vec<&str> = track.effects.iter().map(|fx| fx.kind.name()).collect();
            let text = if names.is_empty() {
                " → no effects".to_string()
            } else {
                format!(" → {}", names.join(" "))
            };
            vec![Span::styled(fit(&text, w), block(theme.dim))]
        }
    }
}
