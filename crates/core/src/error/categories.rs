//! Error categorization and classification utilities

use super::YuhaError;

/// Error severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    /// Informational - operation can continue
    Info,
    /// Warning - minor issue, operation can continue
    Warning,
    /// Error - operation failed but can be retried
    Error,
    /// Critical - operation failed and requires intervention
    Critical,
    /// Fatal - unrecoverable error
    Fatal,
}

/// Error categories for classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// Network-related errors
    Network,
    /// Authentication/authorization errors
    Authentication,
    /// Configuration errors
    Configuration,
    /// Resource errors (memory, disk, etc.)
    Resource,
    /// User input errors
    UserInput,
    /// System/platform errors
    System,
    /// Application logic errors
    Logic,
    /// External service errors
    External,
}

/// Error classification trait
pub trait ErrorClassification {
    /// Get the severity of this error
    fn severity(&self) -> ErrorSeverity;

    /// Get the category of this error
    fn category(&self) -> ErrorCategory;

    /// Check if this error is user-facing
    fn is_user_facing(&self) -> bool;

    /// Check if this error should be logged
    fn should_log(&self) -> bool;

    /// Check if this error should trigger alerts
    fn should_alert(&self) -> bool;
}

impl ErrorClassification for YuhaError {
    fn severity(&self) -> ErrorSeverity {
        match self {
            YuhaError::Transport(transport_err) => match transport_err {
                super::TransportError::ConnectionFailed { .. } => ErrorSeverity::Error,
                super::TransportError::AuthenticationFailed { .. } => ErrorSeverity::Error,
                super::TransportError::NotAvailable { .. } => ErrorSeverity::Critical,
                super::TransportError::ConfigurationError { .. } => ErrorSeverity::Error,
                _ => ErrorSeverity::Error,
            },
            YuhaError::Protocol(protocol_err) => match protocol_err {
                super::ProtocolError::Timeout { .. } => ErrorSeverity::Warning,
                super::ProtocolError::ChannelClosed => ErrorSeverity::Error,
                super::ProtocolError::BufferOverflow { .. } => ErrorSeverity::Critical,
                _ => ErrorSeverity::Error,
            },
            YuhaError::Session(session_err) => match session_err {
                super::SessionError::NotFound { .. } => ErrorSeverity::Warning,
                super::SessionError::MaxSessionsReached { .. } => ErrorSeverity::Critical,
                super::SessionError::Expired { .. } => ErrorSeverity::Warning,
                _ => ErrorSeverity::Error,
            },
            YuhaError::Config(_) => ErrorSeverity::Error,
            YuhaError::Io(_) => ErrorSeverity::Error,
            YuhaError::Clipboard(_) => ErrorSeverity::Warning,
            YuhaError::Browser(_) => ErrorSeverity::Warning,
            YuhaError::Auth(_) => ErrorSeverity::Error,
            YuhaError::Daemon(daemon_err) => match daemon_err {
                super::DaemonError::NotRunning => ErrorSeverity::Error,
                super::DaemonError::AlreadyRunning { .. } => ErrorSeverity::Warning,
                super::DaemonError::ClientLimitReached { .. } => ErrorSeverity::Critical,
                super::DaemonError::ShuttingDown => ErrorSeverity::Info,
                _ => ErrorSeverity::Error,
            },
            YuhaError::Internal { .. } => ErrorSeverity::Fatal,
        }
    }

    fn category(&self) -> ErrorCategory {
        match self {
            YuhaError::Transport(_) => ErrorCategory::Network,
            YuhaError::Protocol(_) => ErrorCategory::Network,
            YuhaError::Session(_) => ErrorCategory::Logic,
            YuhaError::Config(_) => ErrorCategory::Configuration,
            YuhaError::Io(_) => ErrorCategory::System,
            YuhaError::Clipboard(_) => ErrorCategory::System,
            YuhaError::Browser(_) => ErrorCategory::External,
            YuhaError::Auth(_) => ErrorCategory::Authentication,
            YuhaError::Daemon(_) => ErrorCategory::Logic,
            YuhaError::Internal { .. } => ErrorCategory::Logic,
        }
    }

    fn is_user_facing(&self) -> bool {
        match self.severity() {
            ErrorSeverity::Info | ErrorSeverity::Warning => false,
            ErrorSeverity::Error | ErrorSeverity::Critical | ErrorSeverity::Fatal => true,
        }
    }

    fn should_log(&self) -> bool {
        !matches!(self.severity(), ErrorSeverity::Info)
    }

    fn should_alert(&self) -> bool {
        matches!(
            self.severity(),
            ErrorSeverity::Critical | ErrorSeverity::Fatal
        )
    }
}

/// Error metrics for monitoring
#[derive(Debug, Clone)]
pub struct ErrorMetrics {
    pub error_type: String,
    pub category: ErrorCategory,
    pub severity: ErrorSeverity,
    pub timestamp: std::time::SystemTime,
    pub operation: Option<String>,
    pub component: Option<String>,
}

impl ErrorMetrics {
    pub fn from_error(error: &YuhaError) -> Self {
        Self {
            error_type: error.category().to_string(),
            category: match error.category() {
                "network" => ErrorCategory::Network,
                "authentication" => ErrorCategory::Authentication,
                "configuration" => ErrorCategory::Configuration,
                "resource" => ErrorCategory::Resource,
                "user_input" => ErrorCategory::UserInput,
                "system" => ErrorCategory::System,
                "logic" => ErrorCategory::Logic,
                "external" => ErrorCategory::External,
                _ => ErrorCategory::Logic,
            },
            severity: error.severity(),
            timestamp: std::time::SystemTime::now(),
            operation: None,
            component: None,
        }
    }

    pub fn with_operation<S: Into<String>>(mut self, operation: S) -> Self {
        self.operation = Some(operation.into());
        self
    }

    pub fn with_component<S: Into<String>>(mut self, component: S) -> Self {
        self.component = Some(component.into());
        self
    }
}

impl std::fmt::Display for ErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorCategory::Network => write!(f, "network"),
            ErrorCategory::Authentication => write!(f, "authentication"),
            ErrorCategory::Configuration => write!(f, "configuration"),
            ErrorCategory::Resource => write!(f, "resource"),
            ErrorCategory::UserInput => write!(f, "user_input"),
            ErrorCategory::System => write!(f, "system"),
            ErrorCategory::Logic => write!(f, "logic"),
            ErrorCategory::External => write!(f, "external"),
        }
    }
}

impl std::fmt::Display for ErrorSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorSeverity::Info => write!(f, "info"),
            ErrorSeverity::Warning => write!(f, "warning"),
            ErrorSeverity::Error => write!(f, "error"),
            ErrorSeverity::Critical => write!(f, "critical"),
            ErrorSeverity::Fatal => write!(f, "fatal"),
        }
    }
}
