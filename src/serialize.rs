use tokio::io::AsyncWriteExt;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use tracing::{Level, event};
use anyhow::{anyhow, Result, bail};
use walkdir::DirEntry;
use std::collections::HashMap;
use std::fmt::Display;
use std::path::Path;


pub async fn serialize_sources(data: Vec<(DirEntry, HashMap<String, serde_json::Value>)>) -> Result<()> {
    // Transform file names to record names.
    event!(Level::INFO, "Writing Records");
    tokio::fs::create_dir_all("./../content").await?;
    let joins = data.into_iter()
        .map(|(k, v)| (path_to_record(k), v))
        .map(|(k, v)| (k.clone(), tokio::task::spawn(serialize(k, v))));

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
/// `card/fragment/edge.json` becomes `card.fragment.edge.json`. 
fn path_to_record(path: DirEntry) -> String {
    let mut path = path.into_path();
    path.set_extension("json");
    let file_name = path.components()
        .skip(3)
        .fold(
            String::new(), 
            |mut acc, i| { 
                acc.push('.'); 
                acc.push_str(&i.as_os_str().to_string_lossy()); acc 
            }
        ).trim_start_matches('.').to_owned();
    let path = Path::new(".").join("..").join("content").join(file_name);
    path.to_str().unwrap().trim_matches('"').trim_end_matches(['\\', '/']).to_owned()
}

async fn serialize<A: AsRef<Path> + Display>(path: A, map: HashMap<String, serde_json::Value>) -> Result<()> {
    event!(Level::TRACE, path = format!("{}", path), "Writing file.");
    let mut fd = tokio::fs::File::create(path).await?;
    fd.write_all(serde_json::to_string_pretty(&map)?.as_bytes()).await?;
    Ok(())
}
