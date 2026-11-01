//! Drawing the fx page: the chain of the strip under the mixer cursor, its lanes side by side as
//! columns read top to bottom, like the pattern's tracks. Each effect's name is followed by its
//! settings, a line each with a bar, sends have a line each, and along the bottom are the lanes'
//! outs.

use lattice_core::{FxLane, Kind, Locks, MIN_DB, is_feedback};
use lattice_editor::{Editor, FxLine, Strip};
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

/// Width of the line numbers down the left.
const LABEL: usize = 5;
/// Narrowest and widest a lane gets.
const MIN_LANE: usize = 22;
const MAX_LANE: usize = 34;
/// Width of a setting's name.
const PARAM_LABEL: usize = 12;
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
    let lane_width = (width.saturating_sub(LABEL) / lanes.len()).clamp(MIN_LANE, MAX_LANE);
    let locks = match strip {
        Strip::Track(t) => status.locks(t),
        _ => Locks::default(),
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
            FxLine::Fx { effect, .. } => {
                let name = chain[effect].kind.name();
                let rule = "─".repeat(w.saturating_sub(name.chars().count() + 2));
                vec![
                    Span::styled(
                        format!(" {name}"),
                        block(Style::new().fg(colour).add_modifier(Modifier::BOLD)),
                    ),
                    Span::styled(format!(" {rule}"), Style::new().fg(colour)),
                ]
            }
            FxLine::Param { effect, param, .. } => {
                let fx = &chain[effect];
                let info = &fx.params()[param];
                let bar = w - PARAM_LABEL;
                // While it plays, a setting a step has locked shows the lock's value.
                let lock = (locks.iter())
                    .find(|l| l.fx == Some(effect as u8) && u32::from(l.param) == info.id);
                let (label, value, label_style, style) = match lock {
                    Some(lock) => {
                        let mark = Style::new().fg(theme.accent).add_modifier(Modifier::BOLD);
                        (format!(" ◆{}", info.name), lock.value, mark, mark)
                    }
                    None => (
                        format!("  {}", info.name),
                        fx.values[param],
                        name_style(here),
                        value_style(here),
                    ),
                };
                let mut spans = vec![Span::styled(fit(&label, PARAM_LABEL), label_style)];
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
            FxLine::Out => vec![],
        };
        let used: usize = spans.iter().map(|s| s.content.chars().count()).sum();
        spans.push(Span::raw(" ".repeat(lane_width.saturating_sub(used))));
        spans
    };

    for l in (view.fx_top..outs).take(room) {
        let number = if cursor.line == l {
            theme.text
        } else {
            theme.faint
        };
        let mut spans = vec![Span::styled(fit(&format!(" {l:02}"), LABEL), number)];
        for (k, lane) in lanes.iter().enumerate() {
            let line = lane.get(l).copied().unwrap_or(FxLine::Empty);
            spans.extend(cell(k, line, cursor.lane == k && cursor.line == l));
        }
        lines.push(Line::from(spans));
    }

    // Each lane's level into the chain's output, below the longest lane.
    let more = if view.fx_top + room < outs {
        "↓"
    } else {
        " "
    };
    let mut spans = vec![Span::styled(fit(&format!("{more}out"), LABEL), theme.dim)];
    for k in 0..lanes.len() {
        let here = cursor.lane == k && cursor.line == outs;
        let out = chain.lanes()[k].out;
        spans.extend(bars::level(out, lane_width - 1, value_style(here), theme));
        spans.push(Span::raw(" "));
    }
    lines.push(Line::from(spans));
    if chain.is_empty() && lanes.len() == 1 {
        lines.push(Line::raw(""));
        lines.push(Line::styled(
            " no effects yet: :fx-add adds one, and :fx-send B branches into a new lane",
            theme.faint,
        ));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// A strip's name and colour, as the mixer draws it.
fn look(editor: &Editor, theme: &Theme, strip: Strip) -> (String, Color) {
    match strip {
        Strip::Track(i) => (editor.grid.tracks[i].name.to_uppercase(), theme.track(i)),
        Strip::Group(g) => {
            let group = &editor.grid.groups[g];
            (group.name.to_uppercase(), theme.track(group.first))
        }
        Strip::Send => ("SEND".into(), theme.accent),
        Strip::Master => ("MASTER".into(), theme.visual),
    }
}
