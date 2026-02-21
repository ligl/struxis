#[derive(Debug, Clone)]
pub enum BrokerError {
    NotConnected,
    ConnectionFailed(String),
    AdapterError(String),
}

impl std::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BrokerError::NotConnected => write!(f, "broker adapter not connected"),
            BrokerError::ConnectionFailed(msg) => write!(f, "connection failed: {}", msg),
            BrokerError::AdapterError(msg) => write!(f, "adapter error: {}", msg),
        }
    }
}

impl std::error::Error for BrokerError {}
