use crate::{Request, Response};

pub fn handle(_request: Request) -> Response {
    Response {
        status: 200,
        headers: vec![("content-type".into(), "text/plain".into())],
        body: b"Hello, world!".to_vec(),
    }
}
