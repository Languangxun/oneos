use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

pub const DEFAULT_SOCKET: &str = "/run/oneos/oneosd.sock";
pub const SOCKET_ENV: &str = "ONEO_SOCKET";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    Ping,
    Status,
    Poweroff,
    Reboot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: Method,
    #[serde(default)]
    pub params: Value,
}

impl Request {
    pub fn new(id: u64, method: Method) -> Self {
        Self {
            id,
            method,
            params: Value::Null,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<ProtoError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtoError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Status {
    pub version: String,
    pub os: String,
    pub hostname: String,
    pub uptime_secs: u64,
    pub boot_id: String,
}

impl Response {
    pub fn ok<T: Serialize>(id: u64, result: T) -> Self {
        Self {
            id,
            ok: true,
            result: serde_json::to_value(result).ok(),
            error: None,
        }
    }

    pub fn error(id: u64, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            result: None,
            error: Some(ProtoError {
                code: code.into(),
                message: message.into(),
            }),
        }
    }

    pub fn error_message(&self) -> String {
        match &self.error {
            Some(error) => format!("{}: {}", error.code, error.message),
            None => "unknown error".to_string(),
        }
    }
}

pub fn socket_path() -> PathBuf {
    std::env::var_os(SOCKET_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_SOCKET))
}

pub fn connect() -> io::Result<UnixStream> {
    let path = socket_path();
    UnixStream::connect(&path)
        .map_err(|err| io::Error::new(err.kind(), format!("{}: {err}", path.display())))
}

pub fn call(stream: &mut UnixStream, request: &Request) -> io::Result<Response> {
    let mut payload = serde_json::to_vec(request)?;
    payload.push(b'\n');
    stream.write_all(&payload)?;
    stream.flush()?;

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "oneosd closed the connection",
        ));
    }
    Ok(serde_json::from_str(&line)?)
}
