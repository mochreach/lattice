//! The command line's history, kept from one run to the next in `history` beside the log:
//! `~/.local/state/lattice/history`, a command a line, oldest first.

use std::path::PathBuf;

use anyhow::Context;
use lattice_editor::MAX_HISTORY;

use crate::log;

fn path() -> Option<PathBuf> {
    Some(log::path()?.with_file_name("history"))
}

/// The commands run before, or none if there's no history yet.
pub fn load() -> Vec<String> {
    let text = path().and_then(|path| std::fs::read_to_string(path).ok());
    let lines: Vec<&str> = (text.iter().flat_map(|t| t.lines()))
        .filter(|line| !line.is_empty())
        .collect();
    let over = lines.len().saturating_sub(MAX_HISTORY);
    lines.into_iter().skip(over).map(String::from).collect()
}

pub fn save(history: &[String]) -> anyhow::Result<()> {
    let path = path().context("no home folder to keep it in")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    }
    let mut text = history.join("\n");
    text.push('\n');
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))
}
