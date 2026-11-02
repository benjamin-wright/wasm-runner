use askama::Template;

use crate::{Response, libs::responses::text};

#[derive(Template)]
#[template(path = "home.html")]
pub struct Home<'a> {
    pub messages: &'a [String],
}

pub fn html(template: &impl Template) -> Response {
    match template.render() {
        Ok(body) => Response {
            status: 200,
            headers: vec![("content-type".into(), "text/html; charset=utf-8".into())],
            body: body.into_bytes(),
        },
        Err(_) => text(500, "Unable to render page"),
    }
}
