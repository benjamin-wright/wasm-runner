use crate::{
    Request, Response,
    libs::{greetings, responses::text, views},
};

pub fn handle(_request: Request) -> Response {
    match greetings::list() {
        Ok(messages) => views::html(&views::Home { messages: &messages }),
        Err(_) => text(503, "Unable to load greetings"),
    }
}
