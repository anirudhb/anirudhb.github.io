pub mod config;
pub use config::Config;

pub mod process;
pub use process::Processor;

mod frontmatter;
mod render_adapter;
mod util;
