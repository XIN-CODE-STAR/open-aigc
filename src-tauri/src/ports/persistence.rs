use std::error::Error;

use thiserror::Error;

#[derive(Debug, Error)]
#[error("{operation} failed: {source}")]
pub struct PersistenceError {
    operation: &'static str,
    #[source]
    source: Box<dyn Error + Send + Sync>,
}

impl PersistenceError {
    pub fn new(operation: &'static str, source: impl Error + Send + Sync + 'static) -> Self {
        Self {
            operation,
            source: Box::new(source),
        }
    }
}
