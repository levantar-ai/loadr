//! Output/exporter abstraction. Built-in outputs live in `loadr-outputs`;
//! plugin outputs implement the same trait via the plugin bridge.

use async_trait::async_trait;

use crate::aggregate::Snapshot;
use crate::error::EngineError;
use crate::metrics::Sample;
use crate::summary::Summary;

/// A metrics consumer. Methods are called from the aggregator task:
/// `on_samples` per flush batch (~100 ms), `on_snapshot` once per second,
/// `finish` once at the end of the run.
#[async_trait]
pub trait Output: Send {
    fn name(&self) -> &str;

    async fn start(&mut self) -> Result<(), EngineError> {
        Ok(())
    }

    async fn on_samples(&mut self, _samples: &[Sample]) {}

    async fn on_snapshot(&mut self, _snapshot: &Snapshot) {}

    async fn finish(&mut self, _summary: &Summary) {}
}

/// An output that fans samples out to a tokio channel — used to bridge
/// metrics into the web UI and the agent's controller stream.
pub struct ChannelOutput {
    name: String,
    snapshots: tokio::sync::watch::Sender<std::sync::Arc<Snapshot>>,
}

impl ChannelOutput {
    pub fn new(
        name: impl Into<String>,
    ) -> (Self, tokio::sync::watch::Receiver<std::sync::Arc<Snapshot>>) {
        let (tx, rx) = tokio::sync::watch::channel(std::sync::Arc::new(Snapshot::default()));
        (
            ChannelOutput {
                name: name.into(),
                snapshots: tx,
            },
            rx,
        )
    }
}

#[async_trait]
impl Output for ChannelOutput {
    fn name(&self) -> &str {
        &self.name
    }

    async fn on_snapshot(&mut self, snapshot: &Snapshot) {
        let _ = self.snapshots.send(std::sync::Arc::new(snapshot.clone()));
    }
}
