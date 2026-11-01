use crate::{sql::{self, Statement, Value}, Request, Response};

pub fn handle(_request: Request) -> Response {
    let statement = Statement {
        sql: "SELECT message FROM greetings WHERE id = ?1".into(),
        parameters: vec![Value::Integer(1)],
    };
    let (status, message) = match sql::query(&statement).and_then(|cursor| cursor.next()) {
        Ok(Some(row)) => match row.first() {
            Some(Value::Text(message)) => (200, message.clone()),
            _ => (500, "Invalid greeting".into()),
        },
        Ok(None) => (404, "Greeting not found".into()),
        Err(_) => (500, "Unable to read greeting".into()),
    };
    Response { status, headers: vec![], body: message.into_bytes() }
}
