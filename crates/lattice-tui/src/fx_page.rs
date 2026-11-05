//! Drawing the fx page: the chain of the strip under the mixer cursor, its lanes side by side as
//! columns read top to bottom, like the pattern's tracks. Each effect's name is followed by its
//! settings, a line each with a bar, sends have a line each, and along the bottom are the lanes'
//! outs. On a track, its macros are a column left of the lanes, each followed by the settings it
//! turns and how far; a setting a macro turns is marked with the macro's number in its lane, and lit while the cursor
//! is on it in the macros' column.

use lattice_core::knob;
use lattice_core::macros::MACRO_INFO;
use lattice_core::{FxLane, Kind, Locks, MIN_DB, is_feedback};
use lattice_editor::{Editor, FxLine, Strip, target_name};
use lattice_engine::Status;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::bars;
use crate::mixer_page::{flag, route};
use crate::theme::Theme;
use crate::ui::{ViewState, fit};

/// Width of the gutter down the left, which points to the cursor's line.
const LABEL: usize = 5;
/// Narrowest and widest a lane gets.
const MIN_LANE: usize = 22;
const MAX_LANE: usize = 34;
/// Width of a setting's name.
const PARAM_LABEL: usize = 13;
/// Width of the macros' column, and of a macro's name in it.
const MACROS: usize = 34;
const MACRO_LABEL: usize = 18;
/// Lines above the lanes (the rule, a gap and the lanes' letters) and below (the outs).
const ABOVE: usize = 3;
const BELOW: usize = 1;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    editor: &Editor,
    status: &Status,
    theme: &Theme,
    view: &mut ViewState,
) {
    let width = area.width as usize;
    let strip = editor.fx_strip();
    let (name, colour) = look(editor, theme, strip);
    let chain = editor.chain(strip);
    let (lanes, outs) = editor.fx_page_lines();
    let cursor = editor.fx_cursor();
    let macro_lines = editor.macro_lines();
    let macros_width = match strip {
        Strip::Track(_) => MACROS,
        _ => 0,
    };
    let lane_width =
        (width.saturating_sub(LABEL + macros_width) / lanes.len().max(1)).clamp(MIN_LANE, MAX_LANE);
    let locks = match strip {
        Strip::Track(t) => status.locks(t),
        _ => Locks::default(),
    };
    let track_macros = match strip {
        Strip::Track(t) => editor
            .grid
            .tracks
            .get(t)
            .map_or(&[][..], |t| t.macros.as_slice()),
        _ => &[],
    };
    let track_knobs = match strip {
        Strip::Track(t) => (editor.grid.tracks.get(t)).map_or(&[][..], |t| t.knobs.as_slice()),
        _ => &[],
    };
    // The setting the cursor's on in the macros' column, to light in its lane.
    let pointed = match editor.fx_line() {
        FxLine::Target { index, target } if cursor.macros => track_macros
            .get(index)
            .and_then(|m| m.targets.get(target))
            .map(|target| (target.fx, target.param)),
        _ => None,
    };

    // A rule with the strip's name and where its sound goes, as on the mixer.
    let (flag, flag_colour) = flag(editor, strip, theme);
    let title = format!("── {name} ");
    let route = format!(" effects {} ", route(editor, strip));
    let used = 1 + title.chars().count() + 2 + route.chars().count();
    let mut lines = vec![Line::from(vec![
        Span::raw(" "),
        Span::styled(title, Style::new().fg(colour).add_modifier(Modifier::BOLD)),
        Span::styled(
            flag,
            Style::new().fg(flag_colour).add_modifier(Modifier::BOLD),
        ),
        Span::styled("─", theme.faint),
        Span::styled(route, theme.dim),
        Span::styled("─".repeat(width.saturating_sub(used)), theme.faint),
    ])];
    lines.push(Line::raw(""));

    // Each lane's letter, and what feeds it.
    let mut spans = vec![Span::raw(" ".repeat(LABEL))];
    if macros_width > 0 {
        let style = Style::new().fg(colour).add_modifier(Modifier::BOLD);
        spans.push(Span::styled(fit(" macros", macros_width), style));
    }
    for k in 0..lanes.len() {
        let mut from: Vec<String> = (k == 0).then(|| "in".to_string()).into_iter().collect();
        for (l, lane) in lanes.iter().enumerate() {
            for line in lane {
                if let FxLine::Send { to, .. } = *line
                    && to == k
                {
                    let letter = FxLane::letter(l).to_string();
                    if !from.contains(&letter) {
                        from.push(letter);
                    }
                }
            }
        }
        let style = Style::new().fg(colour).add_modifier(Modifier::BOLD);
        spans.push(Span::styled(format!(" {}", FxLane::letter(k)), style));
        spans.push(Span::styled(
            fit(&format!(" ← {}", from.join(" ")), lane_width - 2),
            theme.dim,
        ));
    }
    lines.push(Line::from(spans));

    // As many lines as fit, scrolled to keep the cursor's in view.
    let room = (area.height as usize).saturating_sub(ABOVE + BELOW).max(1);
    if cursor.line < outs {
        if cursor.line < view.fx_top {
            view.fx_top = cursor.line;
        } else if cursor.line >= view.fx_top + room {
            view.fx_top = cursor.line + 1 - room;
        }
    }
    view.fx_top = view.fx_top.min(outs.saturating_sub(room));

    let name_style = |here: bool| {
        if here {
            theme.text.add_modifier(Modifier::BOLD)
        } else {
            theme.dim
        }
    };
    // The value under the cursor in the terminal's own colour, as on the mixer.
    let value_style = |here: bool| {
        if here {
            Style::new().fg(Color::Reset)
        } else {
            Style::new().fg(colour)
        }
    };
    // One lane's line, a lane wide.
    let cell = |lane: usize, line: FxLine, here: bool| -> Vec<Span<'static>> {
        let w = lane_width - 1;
        let block = |style: Style| {
            if here {
                theme.block(Color::Reset)
            } else {
                style
            }
        };
        let mut spans = match line {
            FxLine::Fx { effect, .. } if let Some(fx) = chain.get(effect) => {
                let name = fx.kind.name();
                let rule = "─".repeat(w.saturating_sub(name.chars().count() + 2));
                vec![
                    Span::styled(
                        format!(" {name}"),
                        block(Style::new().fg(colour).add_modifier(Modifier::BOLD)),
                    ),
                    Span::styled(format!(" {rule}"), Style::new().fg(colour)),
                ]
            }
            FxLine::Param { effect, param, .. }
                if let Some(fx) = chain.get(effect)
                    && let Some(info) = fx.params().get(param) =>
            {
                let bar = w - PARAM_LABEL;
                // While it plays, a setting a step has locked shows the lock's value.
                let lock = (locks.iter())
                    .find(|l| l.fx == Some(effect as u8) && u32::from(l.param) == info.id);
                // A macro turning it is marked with its number, and lit while the cursor's on it
                // among the macros.
                let id = (Some(effect as u8), info.id as u8);
                let turned = Style::new().fg(theme.visual);
                let (mark, value, label_style, style) = match lock {
                    Some(lock) => {
                        let mark = Style::new().fg(theme.accent).add_modifier(Modifier::BOLD);
                        (Span::styled(" ◆ ", mark), lock.value, mark, mark)
                    }
                    None if let Some(m) = knob::mark(track_knobs, track_macros, id.0, id.1) => (
                        Span::styled(format!(" {m} "), turned),
                        fx.get(info.id),
                        name_style(here || pointed == Some(id)),
                        value_style(here),
                    ),
                    None => (
                        Span::raw("   "),
                        fx.get(info.id),
                        name_style(here),
                        value_style(here),
                    ),
                };
                let mut spans = vec![
                    mark,
                    Span::styled(fit(info.name, PARAM_LABEL - 3), label_style),
                ];
                spans.extend(bars::param(info, value, bar, style, theme));
                spans
            }
            FxLine::Send { to, level, .. } => {
                let (arrow, colour) = if is_feedback(lane, to) {
                    ("↺", theme.visual)
                } else {
                    ("→", theme.accent)
                };
                let level = if level <= MIN_DB {
                    "off".into()
                } else {
                    format!("{level:+.1} dB")
                };
                let style = Style::new().fg(colour).add_modifier(Modifier::BOLD);
                let text = format!(" {arrow} {}  {level}", FxLane::letter(to));
                vec![Span::styled(text, block(style))]
            }
            // The lane flows on down to its out.
            FxLine::Empty => vec![Span::styled(" │", block(theme.faint))],
            // Lanes have neither: they're the macros' column's. An effect or setting that's gone
            // draws as nothing, until the editor's lines catch up.
            FxLine::Out | FxLine::Macro { .. } | FxLine::Target { .. } | FxLine::Fx { .. } => {
                vec![]
            }
            FxLine::Param { .. } => vec![],
        };
        let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        spans.push(Span::raw(" ".repeat(lane_width.saturating_sub(used))));
        spans
    };

    // One line of the macros' column.
    let macro_cell = |line: FxLine, here: bool| -> Vec<Span<'static>> {
        let Strip::Track(t) = strip else {
            return vec![];
        };
        let Some(track) = editor.grid.tracks.get(t) else {
            return vec![];
        };
        let w = macros_width - 2;
        let mut spans = match line {
            FxLine::Macro { index } if let Some(m) = track.macros.get(index) => {
                // While it plays, a macro a step has locked shows the lock's value.
                let lock = (locks.iter()).find(|l| l.is_macro() && usize::from(l.param) == index);
                let (label, value, label_style, style) = match lock {
                    Some(lock) => {
                        let mark = Style::new().fg(theme.accent).add_modifier(Modifier::BOLD);
                        (
                            format!(" ◆{} {}", index + 1, m.name),
                            lock.value,
                            mark,
                            mark,
                        )
                    }
                    None => (
                        format!(" {} {}", index + 1, m.name),
                        m.value,
                        Style::new().fg(colour).add_modifier(Modifier::BOLD),
                        value_style(here),
                    ),
                };
                let label_style = if here {
                    theme.block(Color::Reset)
                } else {
                    label_style
                };
                let mut spans = vec![Span::styled(fit(&label, MACRO_LABEL), label_style)];
                spans.extend(bars::param(
                    &MACRO_INFO,
                    value,
                    w - MACRO_LABEL,
                    style,
                    theme,
                ));
                spans
            }
            FxLine::Target { index, target }
                if let Some(target) =
                    (track.macros.get(index)).and_then(|m| m.targets.get(target)) =>
            {
                let label = format!("  {}", target_name(track, target));
                let mut spans = vec![Span::styled(fit(&label, MACRO_LABEL), name_style(here))];
                spans.extend(bars::depth(
                    target.depth,
                    w - MACRO_LABEL,
                    value_style(here),
                    theme,
                ));
                spans
            }
            _ if here => vec![Span::styled(" ", theme.block(Color::Reset))],
            _ => vec![],
        };
        let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        spans.push(Span::raw(" ".repeat(macros_width.saturating_sub(used))));
        spans
    };

    for l in (view.fx_top..outs).take(room) {
        // Not numbered, as the pattern's steps are: these lines are a list, not time. An arrow
        // shows the cursor's.
        let gutter = if cursor.line == l { "  ▸" } else { "" };
        let mut spans = vec![Span::styled(fit(gutter, LABEL), theme.text)];
        if macros_width > 0 {
            let line = macro_lines.get(l).copied().unwrap_or(FxLine::Empty);
            spans.extend(macro_cell(line, cursor.macros && cursor.line == l));
        }
        for (k, lane) in lanes.iter().enumerate() {
            let line = lane.get(l).copied().unwrap_or(FxLine::Empty);
            spans.extend(cell(
                k,
                line,
                !cursor.macros && cursor.lane == k && cursor.line == l,
            ));
        }
        lines.push(Line::from(spans));
    }

    // Where the effects would be, above the out they'd go into, so the chain still reads down.
    if chain.is_empty() && lanes.len() == 1 {
        lines.push(Line::styled(
            " no effects yet: :fx-add adds one, and :fx-send B branches into a new lane",
            theme.faint,
        ));
        lines.push(Line::raw(""));
    }
    // Each lane's level into the chain's output, below the longest lane.
    let more = if view.fx_top + room < outs {
        "↓"
    } else {
        " "
    };
    let mut spans = vec![Span::styled(fit(&format!("{more}out"), LABEL), theme.dim)];
    spans.push(Span::raw(" ".repeat(macros_width)));
    for k in 0..lanes.len() {
        let here = !cursor.macros && cursor.lane == k && cursor.line == outs;
        // A lane the chain hasn't got leaves a gap.
        match chain.lanes().get(k) {
            Some(lane) => spans.extend(bars::level(
                lane.out,
                lane_width - 1,
                value_style(here),
                theme,
            )),
            None => spans.push(Span::raw(" ".repeat(lane_width - 1))),
        }
        spans.push(Span::raw(" "));
    }
    lines.push(Line::from(spans));
    if macros_width > 0 && macro_lines.is_empty() {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            " no macros yet: :map NAME on a setting has macro NAME turn it",
            theme.faint,
        ));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// A strip's name and colour, as the mixer draws it.
fn look(editor: &Editor, theme: &Theme, strip: Strip) -> (String, Color) {
    match strip {
        // One that's gone has no name.
        Strip::Track(i) => match editor.grid.tracks.get(i) {
            Some(track) => (track.name.to_uppercase(), theme.track(i)),
            None => (String::new(), theme.track(i)),
        },
        Strip::Group(g) => match editor.grid.groups.get(g) {
            Some(group) => (group.name.to_uppercase(), theme.track(group.first)),
            None => (String::new(), theme.accent),
        },
        Strip::Send => ("SEND".into(), theme.accent),
        Strip::Master => ("MASTER".into(), theme.visual),
    }
}
