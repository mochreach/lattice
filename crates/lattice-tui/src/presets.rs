//! Saved presets, as TOML files in `presets` beside the config, one a preset:
//! `:preset-save cold-iron` writes `presets/cold-iron.toml`, and `:preset cold-iron` reads it.
//! A name's folders are folders there: `:preset-save mine/cold-iron` writes
//! `presets/mine/cold-iron.toml`.

use std::path::PathBuf;

use anyhow::Context;
use lattice_core::{PRESETS, Preset, SavedPreset};

use crate::config;

/// Where the presets are saved: `~/.config/lattice/presets`.
fn dir() -> Option<PathBuf> {
    Some(config::default_path()?.with_file_name("presets"))
}

/// Writes `preset` as `name`, and says where.
pub fn save(name: &str, preset: Preset) -> anyhow::Result<String> {
    let path = dir()
        .context("no home folder to save presets in")?
        .join(format!("{name}.toml"));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    }
    let title = preset.face.title.clone();
    let text = to_toml(preset)?;
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(format!("saved {title} as {name}, in {}", path.display()))
}

/// Reads the preset saved as `name`.
pub fn load(name: &str) -> anyhow::Result<Preset> {
    let path = dir()
        .context("no home folder to load presets from")?
        .join(format!("{name}.toml"));
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!(
                "no preset called {name}; there's {}",
                top(&names()).join(", ")
            )
        }
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    from_toml(&text).with_context(|| format!("in {}", path.display()))
}

fn to_toml(preset: Preset) -> anyhow::Result<String> {
    Ok(toml::to_string(&SavedPreset::from(preset))?)
}

fn from_toml(text: &str) -> anyhow::Result<Preset> {
    let saved: SavedPreset = toml::from_str(text)?;
    Preset::try_from(saved).map_err(anyhow::Error::msg)
}

/// The most folders deep a saved preset is looked for.
const MAX_DEPTH: usize = 4;

/// The presets there are: Lattice's, then those saved, by name, folders and all.
pub fn names() -> Vec<String> {
    let mut saved = vec![];
    if let Some(dir) = dir() {
        find(&dir, "", MAX_DEPTH, &mut saved);
    }
    saved.sort();
    PRESETS.iter().map(|p| p.to_string()).chain(saved).collect()
}

/// Adds the presets saved in `dir`, and in its folders, to `found`, each named after `folder`.
fn find(dir: &std::path::Path, folder: &str, depth: usize, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|entry| entry.path()) {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if path.is_dir() && depth > 0 && !stem.starts_with('.') {
            find(&path, &format!("{folder}{stem}/"), depth - 1, found);
        } else if path.extension().is_some_and(|e| e == "toml") {
            found.push(format!("{folder}{stem}"));
        }
    }
}

/// The folders and presets at the top of `names`, each folder once, with its `/`.
fn top(names: &[String]) -> Vec<String> {
    let mut top: Vec<String> = vec![];
    for name in names {
        let first = match name.split_once('/') {
            Some((folder, _)) => format!("{folder}/"),
            None => name.clone(),
        };
        if !top.contains(&first) {
            top.push(first);
        }
    }
    top
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_go_through_toml() {
        for name in PRESETS {
            let preset = lattice_core::preset(name).unwrap();
            let text = to_toml(preset.clone()).unwrap();
            assert_eq!(from_toml(&text).unwrap(), preset, "{text}");
        }
    }

    #[test]
    fn saved_presets_are_found_in_their_folders() {
        let dir = std::env::temp_dir().join(format!("lattice-presets-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("mine/old")).unwrap();
        for file in [
            "top.toml",
            "mine/cold-iron.toml",
            "mine/old/rust.toml",
            "mine/notes.txt",
        ] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        let mut found = vec![];
        find(&dir, "", MAX_DEPTH, &mut found);
        found.sort();
        assert_eq!(found, ["mine/cold-iron", "mine/old/rust", "top"]);
        std::fs::remove_dir_all(&dir).unwrap();

        let names = ["synth/a", "synth/b", "tuss/c", "top"].map(String::from);
        assert_eq!(top(&names), ["synth/", "tuss/", "top"]);
    }
}
