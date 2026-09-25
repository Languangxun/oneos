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
    SessionStatus,
    SessionStart,
    SessionStop,
    ServiceList,
    ServiceStatus,
    ServiceStart,
    ServiceStop,
    ServiceRestart,
    Logs,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub active: bool,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub unit: String,
    pub load: String,
    pub active: String,
    pub sub: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitParam {
    pub unit: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LogsParam {
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub lines: Option<u32>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_names_are_snake_case() {
        assert_eq!(serde_json::to_string(&Method::Ping).unwrap(), "\"ping\"");
        assert_eq!(
            serde_json::to_string(&Method::ServiceList).unwrap(),
            "\"service_list\""
        );
        assert_eq!(
            serde_json::from_str::<Method>("\"session_start\"").unwrap(),
            Method::SessionStart
        );
    }

    #[test]
    fn request_roundtrip_with_params() {
        let request = Request {
            id: 7,
            method: Method::ServiceStatus,
            params: serde_json::json!({ "unit": "oneosd.service" }),
        };
        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: Request = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.id, 7);
        assert_eq!(decoded.method, Method::ServiceStatus);
        let params: UnitParam = serde_json::from_value(decoded.params).unwrap();
        assert_eq!(params.unit, "oneosd.service");
    }

    #[test]
    fn request_defaults_params_to_null() {
        let decoded: Request = serde_json::from_str(r#"{"id":1,"method":"ping"}"#).unwrap();
        assert_eq!(decoded.params, Value::Null);
    }

    #[test]
    fn response_shapes() {
        let ok = Response::ok(
            1,
            SessionState {
                active: true,
                state: "active".into(),
            },
        );
        let encoded = serde_json::to_value(&ok).unwrap();
        assert_eq!(encoded["ok"], true);
        assert_eq!(encoded["result"]["state"], "active");
        assert!(encoded.get("error").is_none());

        let error = Response::error(2, "bad_params", "missing unit");
        let encoded = serde_json::to_value(&error).unwrap();
        assert_eq!(encoded["ok"], false);
        assert_eq!(encoded["error"]["code"], "bad_params");
        assert!(encoded.get("result").is_none());
    }

    #[test]
    fn logs_param_defaults() {
        let param: LogsParam = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(param.unit.is_none());
        assert!(param.lines.is_none());
    }
}
