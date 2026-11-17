//! Events published by components and received by the journal.

/// A message tagged with its source component.
#[derive(Clone, Debug)]
pub struct Event {
    pub source: String,
    pub message: String,
}
