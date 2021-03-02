use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Config {
    // Basic roots
    roots: RootsConfig,

    // Basic inputs
    inputs: InputsConfig,

    // Lib config
    lib: LibConfig,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RootsConfig {
    /// Source root
    source: String,
    /// Lib root
    lib: String,
    /// Assets root
    assets: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct InputsConfig {
    /// Index page
    ///
    /// If none, defaults to the index.md file in the source root
    index: Option<String>,
    /// Root _keep file
    ///
    /// If none, defaults to the _keep file in the source root
    keep: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LibConfig {
    /// Prelude location
    ///
    /// If none, defaults to the prelude.html file in the lib root
    prelude_location: Option<String>,
    // Style config
    styles: StylesConfig,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct StylesConfig {
    /// Style chunks root
    ///
    /// If none, defaults to the style-chunks folder in the lib root
    chunks_root: Option<String>,
    /// CSS filenames
    ///
    /// Some defaults:
    /// global: defaults to the _global.css file in the style chunks root
    /// *: defaults to the *.css file in the style chunks root
    ///    (e.g. image -> image.css, link -> link.css, etc.)
    ///
    /// Note that nonexistent files are ignored and relative paths are resolved
    /// relative to the style chunks root
    css: HashMap<String, String>,
}
