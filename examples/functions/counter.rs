use crate::{Request, Response, kv};

pub fn handle(_request: Request) -> Response {
    match kv::increment("visits") {
        Ok(visits) => Response {
            status: 200,
            headers: vec![("content-type".into(), "text/plain".into())],
            body: visits.to_string().into_bytes(),
        },
        Err(error) => Response {
            status: 503,
            headers: vec![],
            body: format!("Counter unavailable: {error:?}").into_bytes(),
        },
    }
}
