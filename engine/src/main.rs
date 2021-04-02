use anyhow::Context;
use argh::FromArgs;
use engine::{Config, Processor};
use tokio::{fs::File, io::AsyncReadExt};
use tracing::{event, instrument, Level};
use tracing_subscriber::EnvFilter;

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

#[instrument]
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = argh::from_env::<Args>();

    let format = tracing_subscriber::fmt::format().pretty();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .event_format(format)
        .init();

    event!(Level::INFO, input_filename = ?args.config_filename);
    let cfg = {
        let mut f = File::open(&args.config_filename).await?;
        let mut s = String::new();
        f.read_to_string(&mut s).await?;
        Ok::<_, anyhow::Error>(toml::from_str::<Config>(&s)?)
    }?
    .resolve(
        &args
            .config_filename
            .parent()
            .context("Parent folder of config file")?,
    );
    event!(Level::DEBUG, config = ?cfg);
    let processor = Processor::new(cfg);
    processor.render_toplevel(args.force).await?;

    Ok(())
}
