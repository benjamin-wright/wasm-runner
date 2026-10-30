use crate::{Request, Response};

pub fn handle(request: Request) -> Response {
    Response {
        status: 200,
        headers: vec![("content-type".into(), "application/json".into())],
        body: serde_json::json!({
            "method": request.method,
            "path": request.path,
            "path_params": request.path_params,
            "query": request.query,
        }).to_string().into_bytes(),
    }
}
