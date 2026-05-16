use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: Value,
    pub method: String,
    pub params: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Debug, Serialize)]
pub struct McpError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct McpNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

impl McpResponse {
    pub fn success(id: impl Into<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: impl Into<Value>, code: i64, message: &str) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: id.into(),
            result: None,
            error: Some(McpError {
                code,
                message: message.to_string(),
            }),
        }
    }
}

impl McpNotification {
    pub fn new(method: &str, params: Value) -> Self {
        let params = if params.is_null() { None } else { Some(params) };
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_deserialize() {
        let json = r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"search_local_notes","arguments":{"query":"test"}}}"#;
        let req: McpRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "tools/call");
    }

    #[test]
    fn test_success_response_serialize() {
        let resp = McpResponse::success(1, serde_json::json!({"content": []}));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"result\""));
    }

    #[test]
    fn test_error_response_serialize() {
        let resp = McpResponse::error(1, -32601, "Method not found");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"error\""));
        assert!(json.contains("Method not found"));
    }

    #[test]
    fn test_notification_no_id() {
        let notif = McpNotification::new("notifications/initialized", serde_json::Value::Null);
        let json = serde_json::to_string(&notif).unwrap();
        assert!(!json.contains("\"id\""));
        assert!(json.contains("notifications/initialized"));
    }
}
