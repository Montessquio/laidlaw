use tokio::io::AsyncWriteExt;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use tracing::{Level, event};
use anyhow::{anyhow, Result, bail};
use std::collections::HashMap;
use std::fmt::Display;
use std::path::{Path, PathBuf};

pub async fn serialize_sources(mod_root: &Path, data: Vec<crate::Record>, namespace: Option<String>) -> Result<()> {
    event!(Level::INFO, items = data.len(), "Writing Records");
    
    // Create the output directory if necessary.
    tokio::fs::create_dir_all("./content").await?;

    let joins = data.into_iter()
     // Convert each record to a canonicalized path and filename appropriate for loading by CultSim
     .map(|r| {
        (path_to_record(mod_root, r.meta.source_meta.clone().into_path(), namespace.clone()), r)
     })
     // Asynchronously write all files.
     .map(|(k, v)| {
        (k.clone(), tokio::task::spawn(serialize(k, v.content)))
     });

    async fn join<K, V>((k, v): (K, JoinHandle<Result<V, anyhow::Error>>)) 
        -> (K, std::result::Result<Result<V>, tokio::task::JoinError>) 
        { (k, v.await ) }
    
    // Await all join-futures
    let mut tasks = tokio_stream::iter(joins).map(join);

    let errs = {
        let mut errs: Vec<(String, anyhow::Error)> = Vec::new();
        // Sort join-futures by their results into ok and error vecs.
        while let Some(value) = tasks.next().await {
            match value.await {
                (_, Ok(Ok(_))) => {},
                (k, Ok(Err(e))) => errs.push((k, anyhow!(e))),
                (k, Err(e)) => errs.push((k, anyhow!(e))),
            }
        };
        errs
    };

    if !errs.is_empty() {
        for (file, err) in errs {
            event!(Level::ERROR, file = file, err = err.to_string(), "Parsing Failure");
        }
        bail!("There were errors compiling source files.")
    }

    event!(Level::INFO, "Successfully wrote all records.");

    Ok(())
}

/// Convert a path to a single filename describing the compiled object.
/// 
/// `<MOD_ROOT>/src/content/card/fragment/edge.json` becomes `<MOD_ROOT>/content/card.fragment.edge.json`. 
fn path_to_record(mod_root: &Path, source_path: PathBuf, namespace: Option<String>) -> String {
    let content_root = mod_root.join("content");
    let mut dest_name = source_path.canonicalize().expect("Source Path did not exist!")
        .strip_prefix(mod_root.canonicalize().expect("Mod Root did not exist!"))
        .expect("").to_owned();
        dest_name.set_extension("json");
    let dest_name = dest_name.components()
     .skip(2)
     .fold(
        String::new(),
        |mut acc, i| { 
            acc.push('.'); 
            acc.push_str(&i.as_os_str().to_string_lossy()); acc 
        }
     ).trim_start_matches('.').to_owned();
    let path = if namespace.is_some() {
        Path::new(&content_root).join(&format!("{}.{}", namespace.unwrap(), dest_name))
    } 
    else {
        Path::new(&content_root).join(dest_name)
    };
    path.to_str().unwrap().trim_matches('"').trim_end_matches(['\\', '/']).to_owned()
}

async fn serialize<A: AsRef<Path> + Display>(path: A, map: HashMap<String, serde_json::Value>) -> Result<()> {
    event!(Level::DEBUG, path = format!("{}", path), "Writing file.");
    let mut fd = tokio::fs::File::create(path).await?;
    fd.write_all(serde_json::to_string_pretty(&map)?.as_bytes()).await?;
    Ok(())
}
