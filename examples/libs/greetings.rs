use crate::sql::{self, SqlError, Statement, Value};

pub fn list() -> Result<Vec<String>, SqlError> {
    let cursor = sql::query(&Statement {
        sql: "SELECT message FROM greetings ORDER BY id LIMIT 100".into(),
        parameters: vec![],
    })?;
    let mut messages = Vec::new();
    while let Some(row) = cursor.next()? {
        let Some(Value::Text(message)) = row.into_iter().next() else {
            return Err(SqlError::Storage);
        };
        messages.push(message);
    }
    Ok(messages)
}
