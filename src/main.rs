// Compile all .hjson in `<modroot>/src/content`
// to `<modroot>/content` .json files.
//
// Also, flatten directories.

use tracing::{Level, event};
use tracing_subscriber::FmtSubscriber;
use anyhow::{Result, bail};
use walkdir::DirEntry;
use std::{path::PathBuf, collections::HashMap};
use clap::{Parser};

/// Read files from a directory and convert them
/// to a vector of common dictionary types.
mod deserialize;

mod extensions;

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

#[derive(Debug)]
pub struct Record {
    pub meta: RecordMeta,
    pub content: HashMap<String, serde_json::Value>
}

#[derive(Clone, Debug)]
pub struct RecordMeta {
    /// File Metadata of the source file
    pub source_meta: DirEntry,

    /// The declared type of the file.
    /// This is equal to the first and
    /// only root key in the file.
    pub soft_type: String,
    
    // The number of elements in the
    // file's root element list.
    pub elements: usize,
}

impl std::fmt::Display for RecordMeta {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, 
            "RecordMeta{{path: '{1}', type: '{0}'}}", 
            self.soft_type,
            self.source_meta.path().to_str()
             .unwrap_or(
                &format!(
                    "<InvalidPath '{:?}'>", 
                    self.source_meta.path()
                )
            )
        )
    }
}

impl RecordMeta {
    pub fn new(dir: DirEntry, data: &HashMap<String, serde_json::Value>) -> Self {
        let contents = data.iter().next().expect("Data file must have exactly one top-level element!");
        RecordMeta {
            source_meta: dir,
            soft_type: contents.0.to_owned(),
            elements: contents.1.as_array().expect("The root element in a data file must be a List!").len(),
        }
    }
}

static LONG_ABOUT: &str = r#"Reshape your mod: Laidlaw will transform a variety of object description formats into Cultist Simulator JSON, and applies helpful extensions as well."#;

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
    if let Err(e) = color_eyre::install() { bail!(e) };

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
    let data = extensions::execute_pipeline(data).await?;

    // Save data
    serialize::serialize_sources(&cli.mod_root, data, cli.namespace).await?;

    Ok(())
}
