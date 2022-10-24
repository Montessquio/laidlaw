use std::collections::HashMap;
use anyhow::Result;
use walkdir::DirEntry;

pub async fn execute_pipeline(data: Vec<(DirEntry, HashMap<String, serde_json::Value>)>) -> Result<Vec<(DirEntry, HashMap<String, serde_json::Value>)>> {

    Ok(data)
}