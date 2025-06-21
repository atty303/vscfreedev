//! Error context and enrichment utilities

use super::YuhaError;
use std::collections::HashMap;

/// Error context for enriching errors with additional information
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// Operation being performed when error occurred
    pub operation: String,
    /// Component where the error occurred
    pub component: String,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
    /// Timestamp when error occurred
    pub timestamp: std::time::SystemTime,
}

impl ErrorContext {
    pub fn new<S: Into<String>>(operation: S, component: S) -> Self {
        Self {
            operation: operation.into(),
            component: component.into(),
            metadata: HashMap::new(),
            timestamp: std::time::SystemTime::now(),
        }
    }

    /// Add metadata to the context
    pub fn with_metadata<K, V>(mut self, key: K, value: V) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Add session ID to context
    pub fn with_session_id<S: Into<String>>(self, session_id: S) -> Self {
        self.with_metadata("session_id", session_id)
    }

    /// Add transport type to context
    pub fn with_transport<S: Into<String>>(self, transport_type: S) -> Self {
        self.with_metadata("transport_type", transport_type)
    }

    /// Add host information to context
    pub fn with_host<S: Into<String>>(self, host: S) -> Self {
        self.with_metadata("host", host)
    }
}

/// Error with enriched context
#[derive(Debug)]
pub struct ContextualError {
    pub error: YuhaError,
    pub context: ErrorContext,
}

impl ContextualError {
    pub fn new(error: YuhaError, context: ErrorContext) -> Self {
        Self { error, context }
    }

    /// Get the underlying error
    pub fn into_inner(self) -> YuhaError {
        self.error
    }

    /// Get error with context information
    pub fn with_context(self) -> String {
        format!(
            "[{}:{}] {}: {} (metadata: {:?})",
            self.context.component,
            self.context.operation,
            self.error.category(),
            self.error,
            self.context.metadata
        )
    }
}

impl std::fmt::Display for ContextualError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}:{}] {}: {} (metadata: {:?})",
            self.context.component,
            self.context.operation,
            self.error.category(),
            self.error,
            self.context.metadata
        )
    }
}

impl std::error::Error for ContextualError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.error)
    }
}

/// Trait for adding context to errors
pub trait ErrorContextExt {
    fn with_context(self, context: ErrorContext) -> ContextualError;
    fn with_operation<S: Into<String>>(self, operation: S, component: S) -> ContextualError;
}

impl ErrorContextExt for YuhaError {
    fn with_context(self, context: ErrorContext) -> ContextualError {
        ContextualError::new(self, context)
    }

    fn with_operation<S: Into<String>>(self, operation: S, component: S) -> ContextualError {
        let context = ErrorContext::new(operation, component);
        ContextualError::new(self, context)
    }
}

/// Extension trait for adding context to Result types
pub trait ResultContextExt<T> {
    #[allow(clippy::result_large_err)]
    fn with_context(self, context: ErrorContext) -> Result<T, ContextualError>;
    #[allow(clippy::result_large_err)]
    fn with_operation<S: Into<String>>(
        self,
        operation: S,
        component: S,
    ) -> Result<T, ContextualError>;
}

impl<T> ResultContextExt<T> for Result<T, YuhaError> {
    fn with_context(self, context: ErrorContext) -> Result<T, ContextualError> {
        self.map_err(|e| e.with_context(context))
    }

    fn with_operation<S: Into<String>>(
        self,
        operation: S,
        component: S,
    ) -> Result<T, ContextualError> {
        self.map_err(|e| e.with_operation(operation, component))
    }
}
