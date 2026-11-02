use crate::{
    Request, Response,
    libs::{greetings, responses::text, views},
};

pub fn handle(_request: Request) -> Response {
    match greetings::list() {
        Ok(messages) => views::home(&messages),
        Err(_) => text(503, "Unable to load greetings"),
    }
}
