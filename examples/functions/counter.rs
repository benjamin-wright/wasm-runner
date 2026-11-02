use crate::{Request, Response, kv, libs::responses::text};

pub fn handle(_request: Request) -> Response {
    match kv::increment("visits") {
        Ok(visits) => text(200, visits.to_string()),
        Err(error) => text(503, format!("Counter unavailable: {error:?}")),
    }
}
