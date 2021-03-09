use std::{fs::File, io::Read};

use anyhow::Context;
use argh::FromArgs;
use engine::{Config, Processor};

#[derive(FromArgs)]
/// A simple site generator :)
struct Args {
    #[argh(switch)]
    /// forces rebuild
    force: bool,
    #[argh(positional)]
    /// path to config file
    config_filename: std::path::PathBuf,
}

fn main() -> anyhow::Result<()> {
    let args = argh::from_env::<Args>();
    println!(
        "Input filename: {}",
        args.config_filename.to_str().unwrap_or("unknown")
    );
    let cfg = {
        let mut f = File::open(&args.config_filename)?;
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        Ok::<_, anyhow::Error>(toml::from_str::<Config>(&s)?)
    }?
    .resolve(
        &args
            .config_filename
            .parent()
            .context("Parent folder of config file")?,
    );
    let mut processor = Processor::new(cfg);
    processor.render_toplevel(args.force)?;

    Ok(())
}
