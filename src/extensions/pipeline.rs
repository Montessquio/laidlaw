use futures::future::BoxFuture;
use tokio::sync::mpsc::{Sender, Receiver, channel};
use anyhow::{Result, Error, bail};
use tracing::{event, Level};
use std::{future::Future, sync::atomic::{AtomicUsize, Ordering}};

use super::Record;

static MSPC_CHANNEL_SIZE: usize = 127;

static PIPE_ID_FACTORY: AtomicUsize = AtomicUsize::new(0);

/// A PipelineFunc is any function with the signature
/// ```
/// async fn foo(mut in_stream: Receiver<Record>, mut out_stream: Sender<Record>, mut err_stream: Sender<Error>);
/// ```
type PipelineFunc<R> = fn(Receiver<Record>, Sender<Record>, Sender<Error>) -> R;
// R : Future<Output=()> + Send 
// Bounds are not enforced on type aliases

struct PipelineStep {
    name: String,
    handle: BoxFuture<'static, ()>,
    errors: Receiver<Error>,
}

impl PipelineStep {
    pub fn start(self) -> (String, Receiver<Error>) {
        tokio::task::spawn(self.handle);
        (self.name, self.errors)
    }
}

pub struct Pipeline {
    input: Sender<Record>,
    steps: Vec<PipelineStep>,
    output: Receiver<Record>,
}

impl Pipeline {
    pub fn get_unique() -> usize {
        PIPE_ID_FACTORY.fetch_add(1, Ordering::Relaxed)
    }

    pub fn start(self, input: Vec<Record>) -> (Receiver<Record>, Vec<(String, Receiver<Error>)>) {
        // Cache errors for function return
        let mut errs = Vec::new();
        
        // Start the iterator that feeds the pipeline
        tokio::task::spawn(async move {
            event!(Level::TRACE, items = input.len(), "Spawned pipeline feeder");
            let tx = self.input;
            for r in input {
                if let Err(e) = tx.send(r).await {
                    event!(Level::ERROR, error = e.to_string(), "Error in pipeline root producer")
                }
                event!(Level::DEBUG, "Fed root pipeline");
            }
        });

        // Start each pipeline stage.
        for stage in self.steps {
            errs.push(stage.start());
        }

        // Return handles to the output data
        (self.output, errs)
    }
}
pub struct PipelineBuilder {
    /// The sender to the very first step in the pipeline
    root_producer: Option<Sender<Record>>,
    
    /// Gets modified records from the last step
    last_step_consumer: Option<Receiver<Record>>,

    steps: Vec<PipelineStep>,
}

impl PipelineBuilder {
    pub fn new() -> Self {
        PipelineBuilder { 
            root_producer: None,
            last_step_consumer: None,
            steps: Vec::new(),
         }
    }

    pub fn map(mut self, name: &str, f: PipelineFunc<impl Future<Output = ()> + Send + 'static>) -> Self {
        // First-stage initialization
        if self.root_producer.is_none() {
            let (tx, rx) = channel(MSPC_CHANNEL_SIZE); 
            self.root_producer = Some(tx);
            self.last_step_consumer = Some(rx);            
        }
    
        let from_last_step = self.last_step_consumer.unwrap();
    
        let (to_next_step, rx) = channel(MSPC_CHANNEL_SIZE);
        self.last_step_consumer = Some(rx);

        let (to_errs, from_errs) = channel(MSPC_CHANNEL_SIZE);
        self.steps.push(PipelineStep {
            name: name.to_owned(),
            handle: Box::pin(f(from_last_step, to_next_step, to_errs)),
            errors: from_errs, 
        });
        event!(Level::TRACE, stage = name, MSPC_CHANNEL_SIZE = MSPC_CHANNEL_SIZE, "Mapped new pipeline stage.");

        self
    }

    pub fn build(self) -> Result<Pipeline> {
        if self.steps.is_empty() {
            bail!("A pipeline must have at least one step");
        };
        Ok(Pipeline {
            input: self.root_producer.unwrap(),
            output: self.last_step_consumer.unwrap(),
            steps: self.steps
        })
    }
}