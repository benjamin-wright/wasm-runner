//! Deliberately misbehaving handler for manual server checks, not application logic.
//! Call /misbehave/loop to run forever until the host interrupts execution,
//! /misbehave/trap to panic, or /misbehave/invalid to return an invalid HTTP status.
//! /misbehave/ok returns 1 on every call to demonstrate isolated guest state.
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{Request, Response};

static CALLS: AtomicUsize = AtomicUsize::new(0);

pub fn handle(request: Request) -> Response {
    let mode = request.path_params.first().map(|(_, value)| value.as_str());
    match mode {
        Some("loop") => loop {
            std::hint::spin_loop();
        },
        Some("trap") => panic!("intentional guest trap"),
        Some("invalid") => Response {
            status: 999,
            headers: vec![],
            body: vec![],
        },
        _ => Response {
            status: 200,
            headers: vec![("content-type".into(), "text/plain".into())],
            body: (CALLS.fetch_add(1, Ordering::SeqCst) + 1).to_string().into_bytes(),
        },
    }
}
