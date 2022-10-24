//! # Parser Extensions
//! This module orchestrates the
//! manipulation of the mod files
//! in order to suppport additional
//! functionality.
//! 
//! ## Usage
//! 
//! 
//! ## Creating Extensions
//! To create an extension, implement a
//! [pipeline::PipelineFunc] function.
//! Then, include the function as a step
//! in a [pipeline::PipelineBuilder].
use anyhow::{Result, bail};
use tracing::{event, Level};

use crate::Record;

use self::pipeline::PipelineBuilder;
mod pipeline;

mod recipescript; // DSL for describing complex recipe behavior
mod quicktype;
mod strongtype;

pub async fn execute_pipeline(input: Vec<Record>) -> Result<Vec<Record>> {
    event!(Level::INFO, "Applying Transforms");
    let pipeline = PipelineBuilder::new()
     .map("strongtype", strongtype::pipe)
     .build()?;

    event!(Level::TRACE, "Starting Transform Extension Pipeline");
    let (mut rx, errs) = pipeline.start(input);

    let mut records: Vec<Record> = Vec::new();
    while let Some(record) = rx.recv().await {
        event!(Level::DEBUG, "Got record from pipeline.");
        records.push(record);
    }
    event!(Level::TRACE, records=records.len(), "Collected output records.");

    let mut has_errs = false;
    for (origin, mut err_rx) in errs {
        while let Some(e) = err_rx.recv().await {
            has_errs = true;
            event!(Level::ERROR, stage = origin, error = e.to_string(), "Error executing pipeline stage.")
        }
    }
    if has_errs {
        bail!("Pipeline completed with errors.")
    }
    Ok(records)
}