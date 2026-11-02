use crate::Response;

pub fn text(status: u16, body: impl Into<String>) -> Response {
    Response {
        status,
        headers: vec![("content-type".into(), "text/plain; charset=utf-8".into())],
        body: body.into().into_bytes(),
    }
}
