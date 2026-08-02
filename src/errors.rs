use serde_json::{json, Value};
use std::time::Duration;
use tokio::sync::oneshot;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    NotConnected,
    ConnectionFailed(String),
    ConnectionClosed(String),
    SendFailed(String),
    ParseError(serde_json::Error),
    RequestTimeout(Duration),
    ApiResponse(serde_json::Value),
    OneshotRecvError(oneshot::error::RecvError),
    IoError(std::io::Error),
    TauriError(String),
    Other(String),
}

impl Error {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::NotConnected => "NotConnected",
            Self::ConnectionFailed(_) => "ConnectionFailed",
            Self::ConnectionClosed(_) => "ConnectionClosed",
            Self::SendFailed(_) => "SendFailed",
            Self::ParseError(_) => "ParseError",
            Self::RequestTimeout(_) => "RequestTimeout",
            Self::ApiResponse(_) => "ApiResponse",
            Self::OneshotRecvError(_) => "OneshotRecvError",
            Self::IoError(_) => "IoError",
            Self::TauriError(_) => "TauriError",
            Self::Other(_) => "Other",
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "type": self.kind(),
            "text": self.to_string()
        })
    }
}


impl From<String> for Error {
    fn from(s: String) -> Self {
        Error::Other(s)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::ParseError(e)
    }
}

impl From<oneshot::error::RecvError> for Error {
    fn from(e: oneshot::error::RecvError) -> Self {
        Error::OneshotRecvError(e)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::NotConnected => f.write_str("Not connected"),
            Error::ConnectionFailed(e) => f.write_str(e),
            Error::ConnectionClosed(e) => f.write_str(e),
            Error::SendFailed(e) => f.write_str(e),
            Error::ParseError(e) => e.fmt(f),
            Error::RequestTimeout(d) => write!(f, "{d:?}"),
            Error::ApiResponse(json) => write!(f, "{json}"),
            Error::OneshotRecvError(e) => e.fmt(f),
            Error::IoError(e) => e.fmt(f),
            Error::TauriError(e) => f.write_str(e),
            Error::Other(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for Error {}

pub type ClientResult<T> = Result<T, Error>;
