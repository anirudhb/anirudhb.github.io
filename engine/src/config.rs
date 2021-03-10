use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::util::PathHelper;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    // Basic roots
    pub roots: RootsConfig,
    // Basic inputs
    pub inputs: Option<InputsConfig>,
    // Lib config
    pub lib: Option<LibConfig>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ResolvedConfig {
    // Basic roots
    pub roots: ResolvedRootsConfig,
    // Basic inputs
    pub inputs: ResolvedInputsConfig,
    // Lib config
    pub lib: ResolvedLibConfig,
}

impl Config {
    pub fn resolve(self, config_folder: &Path) -> ResolvedConfig {
        let roots = self.roots.resolve(config_folder);
        let inputs = self
            .inputs
            .unwrap_or_default()
            .resolve(&roots.source, config_folder);
        let lib = self
            .lib
            .unwrap_or_default()
            .resolve(&roots.lib, config_folder);
        ResolvedConfig { roots, inputs, lib }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RootsConfig {
    /// Source root
    pub source: PathBuf,
    /// Lib root
    pub lib: PathBuf,
    /// Assets root
    pub assets: PathBuf,
    /// Output root
    pub output: PathBuf,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ResolvedRootsConfig {
    /// Source root
    pub source: PathBuf,
    /// Lib root
    pub lib: PathBuf,
    /// Assets root
    pub assets: PathBuf,
    /// Output root
    pub output: PathBuf,
}

impl RootsConfig {
    pub fn resolve(self, config_location: &Path) -> ResolvedRootsConfig {
        ResolvedRootsConfig {
            source: self
                .source
                .maybe_suffix(config_location)
                .maybe_canonicalize(),
            lib: self.lib.maybe_suffix(config_location).maybe_canonicalize(),
            assets: self
                .assets
                .maybe_suffix(config_location)
                .maybe_canonicalize(),
            output: self
                .output
                .maybe_suffix(config_location)
                .maybe_canonicalize(),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct InputsConfig {
    /// Index page
    ///
    /// If none, defaults to the index.md file in the source root
    pub index: Option<PathBuf>,
    /// Root _keep file
    ///
    /// If none, defaults to the _keep file in the source root
    pub keep: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ResolvedInputsConfig {
    /// Index page
    pub index: PathBuf,
    /// Root _keep file
    pub keep: PathBuf,
}

impl InputsConfig {
    pub fn resolve(self, source_root: &Path, config_folder: &Path) -> ResolvedInputsConfig {
        ResolvedInputsConfig {
            index: self
                .index
                .map(|x| x.maybe_suffix(config_folder))
                .unwrap_or_else(|| source_root.join("index.md"))
                .maybe_canonicalize(),
            keep: self
                .keep
                .map(|x| x.maybe_suffix(config_folder))
                .unwrap_or_else(|| source_root.join("_keep.md"))
                .maybe_canonicalize(),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct LibConfig {
    /// Prelude location
    ///
    /// If none, defaults to the prelude.html file in the lib root
    pub prelude_location: Option<PathBuf>,
    // Style config
    pub styles: StylesConfig,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ResolvedLibConfig {
    /// Prelude location
    pub prelude_location: PathBuf,
    // Style config
    pub styles: ResolvedStylesConfig,
}

impl LibConfig {
    pub fn resolve(self, lib_root: &Path, config_folder: &Path) -> ResolvedLibConfig {
        ResolvedLibConfig {
            prelude_location: self
                .prelude_location
                .map(|x| x.maybe_suffix(config_folder))
                .unwrap_or_else(|| lib_root.join("prelude.html"))
                .maybe_canonicalize(),
            styles: self.styles.resolve(lib_root, config_folder),
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct StylesConfig {
    /// Style chunks root
    ///
    /// If none, defaults to the style-chunks folder in the lib root
    pub chunks_root: Option<PathBuf>,
    /// CSS filenames
    ///
    /// Some defaults:
    /// global: defaults to the _global.css file in the style chunks root
    /// *: defaults to the *.css file in the style chunks root
    ///    (e.g. image -> image.css, link -> link.css, etc.)
    ///
    /// Note that nonexistent files are ignored and relative paths are resolved
    /// relative to the style chunks root
    pub css: HashMap<String, PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ResolvedStylesConfig {
    /// Style chunks root
    pub chunks_root: PathBuf,
    /// CSS filenames
    pub css: HashMap<String, PathBuf>,
}

impl StylesConfig {
    pub fn resolve(self, lib_root: &Path, config_folder: &Path) -> ResolvedStylesConfig {
        ResolvedStylesConfig {
            chunks_root: self
                .chunks_root
                .map(|x| x.maybe_suffix(config_folder))
                .unwrap_or_else(|| lib_root.join("style-chunks"))
                .maybe_canonicalize(),
            css: self
                .css
                .into_iter()
                .map(|(k, v)| (k, v.maybe_suffix(config_folder).maybe_canonicalize()))
                .collect(),
        }
    }
}
