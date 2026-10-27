use crate::{Request, Response};

pub fn handle(request: Request) -> Response {
    match serde_json::from_slice::<serde_json::Value>(&request.body) {
        Ok(value) => Response {
            status: 200,
            headers: vec![("content-type".into(), "application/json".into())],
            body: value.to_string().into_bytes(),
        },
        Err(_) => Response {
            status: 400,
            headers: vec![],
            body: b"Invalid JSON".to_vec(),
        },
    }
}
