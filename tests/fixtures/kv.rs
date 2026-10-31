use crate::{kv::{self, KvError, Value}, Request, Response};

pub fn handle(request: Request) -> Response {
    let body = match request.path.as_str() {
        "/check" => {
            assert!(kv::get("absent").unwrap().is_none());
            for value in [Value::Bytes(vec![]), Value::Bytes(vec![0, 255]), Value::Text(String::new()), Value::Text("日本語 🦀".into()), Value::Integer(i64::MIN), Value::Integer(i64::MAX)] {
                kv::set("value", &value).unwrap();
                assert_eq!(format!("{:?}", kv::get("value").unwrap().unwrap()), format!("{value:?}"));
            }
            assert!(matches!(kv::increment("value"), Err(KvError::Overflow)));
            assert!(matches!(kv::get("value").unwrap(), Some(Value::Integer(i64::MAX))));
            kv::set("value", &Value::Text("1".into())).unwrap();
            assert!(matches!(kv::increment("value"), Err(KvError::TypeMismatch)));
            assert!(matches!(kv::get("value").unwrap(), Some(Value::Text(_))));
            assert!(kv::delete("value").unwrap());
            assert!(!kv::delete("value").unwrap());
            assert!(matches!(kv::get(""), Err(KvError::InvalidKey)));
            assert!(matches!(kv::set(&"é".repeat(513), &Value::Integer(0)), Err(KvError::InvalidKey)));
            kv::set(&"é".repeat(512), &Value::Bytes(vec![0; 1024 * 1024])).unwrap();
            assert!(matches!(kv::set("large", &Value::Text("é".repeat(512 * 1024 + 1))), Err(KvError::ValueTooLarge)));
            "ok".to_owned()
        }
        "/trap" => { kv::increment("trap").unwrap(); panic!("after accepted write"); }
        "/timeout" => { kv::increment("timeout").unwrap(); loop { std::hint::spin_loop(); } }
        "/read-trap" => format!("{:?}", kv::get("trap").unwrap()),
        "/read-timeout" => format!("{:?}", kv::get("timeout").unwrap()),
        "/disabled" => {
            assert!(matches!(kv::get("count"), Err(KvError::Disabled)));
            assert!(matches!(kv::set("count", &Value::Integer(1)), Err(KvError::Disabled)));
            assert!(matches!(kv::delete("count"), Err(KvError::Disabled)));
            assert!(matches!(kv::increment("count"), Err(KvError::Disabled)));
            "disabled".to_owned()
        }
        _ => kv::increment("count").unwrap().to_string(),
    };
    Response { status: 200, headers: vec![], body: body.into_bytes() }
}
