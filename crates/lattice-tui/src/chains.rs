//! Saved chains of effects, as TOML files in `chains` beside the config, one a chain:
//! `:chain-save wash` writes `chains/wash.toml`, and `:chain-load wash` reads it.

use std::path::PathBuf;

use anyhow::Context;
use lattice_core::{FxChain, PRESETS, SavedChain};

use crate::config;

/// Where the chains are saved: `~/.config/lattice/chains`.
fn dir() -> Option<PathBuf> {
    Some(config::default_path()?.with_file_name("chains"))
}

/// Writes `chain` as `name`, and says where.
pub fn save(name: &str, chain: FxChain) -> anyhow::Result<String> {
    let dir = dir().context("no home folder to save chains in")?;
    std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
    let path = dir.join(format!("{name}.toml"));
    let text = to_toml(chain)?;
    std::fs::write(&path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(format!("saved the chain as {name}, in {}", path.display()))
}

/// Reads the chain saved as `name`.
pub fn load(name: &str) -> anyhow::Result<FxChain> {
    let path = dir()
        .context("no home folder to load chains from")?
        .join(format!("{name}.toml"));
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            anyhow::bail!("no chain called {name}; there's {}", names().join(", "))
        }
        Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
    };
    from_toml(&text).with_context(|| format!("in {}", path.display()))
}

fn to_toml(chain: FxChain) -> anyhow::Result<String> {
    Ok(toml::to_string(&SavedChain::from(chain))?)
}

fn from_toml(text: &str) -> anyhow::Result<FxChain> {
    let saved: SavedChain = toml::from_str(text)?;
    FxChain::try_from(saved).map_err(anyhow::Error::msg)
}

/// The chains there are: Lattice's, then those saved, by name.
fn names() -> Vec<String> {
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
    fn a_branching_chain_goes_through_toml() {
        for name in PRESETS {
            let chain = lattice_core::preset(name).unwrap();
            let text = to_toml(chain.clone()).unwrap();
            assert_eq!(from_toml(&text).unwrap(), chain, "{text}");
        }
        let plain = FxChain::from(vec![lattice_core::Fx::new(lattice_core::FxKind::Comp)]);
        let text = to_toml(plain.clone()).unwrap();
        assert!(!text.contains("lanes"), "{text}");
        assert_eq!(from_toml(&text).unwrap(), plain);
    }
}
