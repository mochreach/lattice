//! Saved presets, as TOML files in `presets` beside the config, one a preset:
//! `:preset-save cold-iron` writes `presets/cold-iron.toml`, and `:preset cold-iron` reads it.

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
    let dir = dir().context("no home folder to save presets in")?;
    std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
    let path = dir.join(format!("{name}.toml"));
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
            anyhow::bail!("no preset called {name}; there's {}", names().join(", "))
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

/// The presets there are: Lattice's, then those saved, by name.
pub fn names() -> Vec<String> {
    let mut saved: Vec<String> = dir()
        .and_then(|dir| std::fs::read_dir(dir).ok())
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let stem = path.file_stem()?.to_str()?.to_string();
            (path.extension()? == "toml").then_some(stem)
        })
        .collect();
    saved.sort();
    PRESETS.iter().map(|p| p.to_string()).chain(saved).collect()
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
}
