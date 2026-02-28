use std::fmt;

#[derive(Debug)]
pub enum ScannerError {
    Connection(String),
    Timeout(String),
    Api(String),
    Parse(String),
    Config(String),
}

impl fmt::Display for ScannerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScannerError::Connection(msg) => write!(f, "Connection error: {msg}"),
            ScannerError::Timeout(msg) => write!(f, "Timeout: {msg}"),
            ScannerError::Api(msg) => write!(f, "API error: {msg}"),
            ScannerError::Parse(msg) => write!(f, "Parse error: {msg}"),
            ScannerError::Config(msg) => write!(f, "Config error: {msg}"),
        }
    }
}

impl std::error::Error for ScannerError {}
