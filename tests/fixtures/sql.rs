use crate::{sql::{self, SqlError, Statement, Value}, Request, Response};

fn statement(sql: &str) -> Statement { Statement { sql: sql.into(), parameters: vec![] } }

pub fn handle(request: Request) -> Response {
    let body = match request.path.as_str() {
        "/disabled" => {
            assert!(matches!(sql::execute(&statement("INVALID")), Err(SqlError::Disabled)));
            assert!(matches!(sql::query(&statement("SELECT 1")), Err(SqlError::Disabled)));
            assert!(matches!(sql::batch(&[]), Err(SqlError::Disabled)));
            "disabled".into()
        }
        "/schema" => sql::query(&statement("SELECT * FROM counters LIMIT 0")).unwrap().columns().join(","),
        "/check" => {
            let values = vec![Value::Null, Value::Integer(i64::MAX), Value::Real(1.5), Value::Text("日本語".into()), Value::Bytes(vec![0, 255])];
            let rows = sql::query(&Statement { sql: "SELECT ?1, ?2, ?3, ?4, ?5".into(), parameters: values.clone() }).unwrap();
            assert_eq!(format!("{:?}", rows.next().unwrap().unwrap()), format!("{values:?}"));
            assert!(sql::batch(&[statement("UPDATE counters SET value = value + 100 WHERE id = 1"), statement("INSERT INTO counters(id, value) VALUES (1, 0)")]).is_err());
            assert!(matches!(sql::query(&statement("SELECT * FROM _wr_migrations")), Err(SqlError::Denied)));
            "ok".into()
        }
        "/scan" => {
            let cursor = sql::query(&statement("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x < 5000) SELECT x FROM n")).unwrap();
            let mut count = 0;
            while cursor.next().unwrap().is_some() { count += 1; }
            count.to_string()
        }
        "/cursor-write" => {
            let cursor = sql::query(&statement("SELECT value FROM counters WHERE id = 1")).unwrap();
            sql::execute(&statement("UPDATE counters SET value = value + 1 WHERE id = 1")).unwrap();
            match cursor.next().unwrap().unwrap()[0] { Value::Integer(value) => value.to_string(), _ => unreachable!() }
        }
        "/nested-limit" => {
            let _first = sql::query(&statement("SELECT 1 UNION ALL SELECT 2")).unwrap();
            assert!(matches!(sql::query(&statement("SELECT 3")), Err(SqlError::Limit)));
            "ok".into()
        }
        "/cursor-limit" => {
            let cursors: Vec<_> = (0..4).map(|_| sql::query(&statement("SELECT 1 UNION ALL SELECT 2")).unwrap()).collect();
            assert!(matches!(sql::query(&statement("SELECT 3")), Err(SqlError::Limit)));
            drop(cursors);
            assert!(sql::query(&statement("SELECT 4")).is_ok());
            "ok".into()
        }
        "/cursor-trap" => {
            let cursors: Vec<_> = (0..4).map(|_| sql::query(&statement("SELECT 1 UNION ALL SELECT 2")).unwrap()).collect();
            std::mem::forget(cursors);
            panic!("with live cursors");
        }
        "/cursor-timeout" => {
            let cursors: Vec<_> = (0..4).map(|_| sql::query(&statement("SELECT 1 UNION ALL SELECT 2")).unwrap()).collect();
            std::mem::forget(cursors);
            loop { std::hint::spin_loop(); }
        }
        "/timeout" => { let _ = sql::query(&statement("WITH RECURSIVE n(x) AS (VALUES(0) UNION ALL SELECT x+1 FROM n) SELECT SUM(x) FROM n")); unreachable!(); }
        path => {
            if path != "/read" {
                sql::execute(&statement("UPDATE counters SET value = value + 1 WHERE id = 1")).unwrap();
            }
            if path == "/trap" { panic!("after committed SQL write"); }
            match sql::query(&statement("SELECT value FROM counters WHERE id = 1")).unwrap().next().unwrap().unwrap()[0] {
                Value::Integer(value) => value.to_string(),
                _ => panic!("expected integer"),
            }
        }
    };
    Response { status: 200, headers: vec![], body: body.into_bytes() }
}
