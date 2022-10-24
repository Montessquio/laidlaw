//! Define additional object schema that
//! extends or constrains existing data types.
//!
//! Takes in any softly-typed object
//! and emits a hard-typed object that
//! conforms to one of the Cultist
//! Simulator data types.
//! 
//! This extension passes base types
//! through untouched.
use tracing::{event, Level};
use anyhow::{Error, Result, bail};
use tokio::sync::mpsc::{Sender, Receiver};
use crate::{Record, extensions::pipeline::Pipeline};

//pub mod aspect;

pub async fn pipe(mut input: Receiver<Record>, output: Sender<Record>, errors: Sender<Error>) {
    let unique = Pipeline::get_unique();
    event!(Level::TRACE, extension="strongtype", instance=unique, "Initialized pipe.");
    while let Some(r) = input.recv().await {
        event!(Level::DEBUG, extension="strongtype", instance=unique, "Got record from pipe.");
        let m = r.meta.clone();
        match execute_item(r) {
            Ok(records) => {
                for record in records { 
                    if let Err(e) = output.send(record).await { 
                        errors.send(e.into()).await.expect("Double Fault"); 
                    } else {
                        event!(Level::DEBUG, extension="strongtype", instance=unique, "Sent record to pipe.");
                    }
                }
            },
            Err(e) => { 
                if let Err(e) = errors.send(e.context(format!("In source file: '{}'", m))).await {
                    errors.send(e.into()).await.expect("Double Fault");
                }
            },
        };
    };
}

fn execute_item(r: Record) -> Result<Vec<Record>> {
    match r.meta.soft_type.as_str() {
        /* Base Types */
        // Pass-through for base types.
        "decks"     |
        "elements"  |
        "legacies"  |
        "recipes"   |
        "verbs"     |
        "endings" => Ok(vec![r]),

        /* Derived Types */
        //"aspects" => aspect::parse(r),
        _ => bail!("Unknown type: {}", r.meta.soft_type),
    }
}