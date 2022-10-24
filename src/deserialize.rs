use anyhow::anyhow;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use tracing::{event, Level};
use anyhow::{Result, bail};
use walkdir::{WalkDir, DirEntry};
use std::collections::HashMap;
use std::path::{PathBuf, Path};

async fn deserialize_hjson(path: PathBuf) -> Result<HashMap<String, serde_json::Value>> {
    let mut ret: HashMap<String, serde_json::Value> = HashMap::new();
    let deser: HashMap<String, nu_json::Value> = nu_json::from_str(&tokio::fs::read_to_string(path).await?)?;
    for (k, v) in deser {
        ret.insert(k, map_value(v));
    }
    Ok(ret)
}

async fn deserialize_json(path: PathBuf) -> Result<HashMap<String, serde_json::Value>> {
    Ok(serde_json::from_str(&tokio::fs::read_to_string(path).await?)?)
}

// Conversion between hjson and json types is trivial,
// but not included. Here it's defined explicitly.
fn map_value(hjson: nu_json::Value) -> serde_json::Value {
    use nu_json::Value::*;
    match hjson {
        Null => serde_json::Value::Null,
        Bool(x) => serde_json::Value::Bool(x),
        I64(x) => serde_json::Value::Number(x.into()),
        U64(x) => serde_json::Value::Number(x.into()),
        F64(x) => serde_json::Value::Number(serde_json::Number::from_f64(x).expect("f64 -> f64 conversion failed")),
        String(x) => serde_json::Value::String(x.trim_end_matches(',').to_owned()),
        Array(x) => serde_json::Value::Array(x.into_iter().map(map_value).collect()),
        Object(x) => {
            let mut ret = serde_json::Map::new();
            for (k, v) in x.into_iter() {
                ret.insert(k, map_value(v));
            }
            serde_json::Value::Object(ret)
        },
    }
}


pub async fn deserialize_sources<A: AsRef<Path>>(source_path: A) -> Result<Vec<(DirEntry, HashMap<String, serde_json::Value>)>> {
        
    // Enumerate all .hjson and .json files and their paths.
    let (json_data, hjson_data) = {
        let mut json_data: Vec<DirEntry> = Vec::new();
        let mut hjson_data: Vec<DirEntry> = Vec::new();
        for entry in WalkDir::new(source_path) {
            let entry = entry.unwrap();
            let ext = entry.path()
                .extension()
                .map_or(
                    "", 
                    |s| (*s).to_str().unwrap_or("")
                );
            if ext == "json" {
                json_data.push(entry);
            } 
            else if ext == "hjson" {
                hjson_data.push(entry);
            }
        }
        (json_data, hjson_data)
    };

    event!(Level::INFO, json = json_data.len(), hjson = hjson_data.len(), "Finished Enumerating Sources.");

    // Parse all .hjson and .json files into a common data structure.
    let joins  = {
        let j = json_data.into_iter().map(|path| (path.clone(), tokio::task::spawn(deserialize_json(path.into_path()))));
        let h = hjson_data.into_iter().map(|path| (path.clone(), tokio::task::spawn(deserialize_hjson(path.into_path()))));
        j.chain(h)
    };

    type AnnotatedJoin<K, V> = (DirEntry, JoinHandle<Result<HashMap<K, V>, anyhow::Error>>);
    async fn join<K, V>((k, v): AnnotatedJoin<K, V>) 
        -> (DirEntry, std::result::Result<Result<HashMap<K, V>>, tokio::task::JoinError>) 
        { (k, v.await ) }
    
    // Await all join-futures
    let mut tasks = tokio_stream::iter(joins).map(join);

    let (errs, data) = {
        let mut errs: Vec<(DirEntry, anyhow::Error)> = Vec::new();
        let mut data: Vec<(DirEntry, HashMap<String, serde_json::Value>)> = Vec::new();
        // Sort join-futures by their results into ok and error vecs.
        while let Some(value) = tasks.next().await {
            match value.await {
                (k, Ok(Ok(v))) => data.push((k, v)),
                (k, Ok(Err(e))) => errs.push((k, anyhow!(e))),
                (k, Err(e)) => errs.push((k, anyhow!(e))),
            }
        };
        (errs, data)
    };

    if !errs.is_empty() {
        for (file, err) in errs {
            event!(Level::ERROR, file = file.path().display().to_string(), err = err.to_string(), "Parsing Failure");
        }
        bail!("There were errors compiling source files.")
    }

    event!(Level::INFO, "Successfully deserialized all sources.");

    Ok(data)
}