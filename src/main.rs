// Compile all .hjson in `<modroot>/src/content`
// to `<modroot>/content` .json files.
//
// Also, flatten directories.

use tracing::{Level, event};
use tracing_subscriber::FmtSubscriber;
use anyhow::{Result, bail};
use std::path::PathBuf;
use clap::{Parser};

/// Read files from a directory and convert them
/// to a vector of common dictionary types.
mod deserialize;

/// Take parsed data and do stuff to it.
mod processing;

/// Save vectors of common dictionary types
/// to files as JSON.
mod serialize;

/*
Overall program control flow:

1. Enumerate all source files in `modroot/src/content`
2. Deserialize all `.hjson` and `.json` files into internal data
3. Apply data transformations.
4. Serialize all data into `.json` files.
5. Flatten and save files.
*/

static LONG_ABOUT: &str = r#""#;

#[derive(Parser, Debug)]
#[command(author, version, about, about = "Reshape your mod: Laidlaw will transform a variety of object description formats into Cultist Simulator JSON, and applies helpful extensions as well.", long_about = LONG_ABOUT)]
struct Args {
    #[arg(help = "The path to the directory containing your mod's `synopsis.json`. Your content files should be in `<MOD_ROOT>/src/content/`")]
    mod_root: PathBuf,

    #[arg(short, long, help = "The namespace your mod occupies. This will be prepended to the name of each output source file along with a dot.")]
    namespace: Option<String>,

    #[arg(short, long, action = clap::ArgAction::Count, conflicts_with = "quiet", help = "Increase log output. Use multiple times to further increase verbosity.")]
    verbose: u8,

    #[arg(short, long, action = clap::ArgAction::Count, conflicts_with = "verbose", help = "Reduce log output. Use multiple times to further decrease verbosity.")]
    quiet: u8,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Args::parse();

    let level = if cli.verbose > 0 {
        match cli.verbose {
            0 => Level::INFO,
            1 => Level::DEBUG,
            2.. => Level::TRACE,
        }
    }
    else if cli.quiet > 0{
        match cli.quiet {
            0 | 3.. => Level::INFO,
            1 => Level::WARN,
            2 => Level::ERROR,
        }
    }
    else {
        Level::INFO
    };

    // Quiet > 2 means be totally silent - panics only.
    if cli.quiet <= 3 {
        let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .finish();
        tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");
    }

    std::env::set_current_dir(&cli.mod_root)?;

    // Read data from sources.
    let source_dir = (&cli.mod_root).clone().join("src").join("content");
    if !source_dir.exists() {
        let mut source_dir = (&cli.mod_root).clone().canonicalize()?.to_str().unwrap().to_owned();
        source_dir.push_str("src/content/");
        event!(Level::ERROR, expected = source_dir, "Source content directory did not exist!");
        bail!("source content directory not found");
    }

    let data = deserialize::deserialize_sources(&source_dir).await?;

    // Manipulate data
    let data = processing::execute_pipeline(data).await?;

    // Save data
    serialize::serialize_sources(data).await?;

    Ok(())
}
