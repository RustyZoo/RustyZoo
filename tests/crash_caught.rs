//! A panic that is caught never asks for a crash report; one that ends the run asks once.
use std::sync::atomic::{AtomicUsize, Ordering};

static ASKED: AtomicUsize = AtomicUsize::new(0);

#[test]
fn a_caught_panic_does_not_ask_and_an_ending_one_asks_once() {
    let dir = std::env::temp_dir().join(format!("crash-caught-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let ask: fn(&str) -> Option<bool> = |_| {
        ASKED.fetch_add(1, Ordering::SeqCst);
        Some(false)
    };
    let dsn = Some("https://k@127.0.0.1:9/1");
    assert!(rustyzoo::crash::install(
        dsn,
        Some(dir.clone()),
        std::path::Path::new("/x"),
        ask
    ));
    assert!(std::panic::catch_unwind(|| panic!("decoder")).is_err());
    assert_eq!(ASKED.load(Ordering::SeqCst), 0, "a caught panic asked");
    let ended = std::panic::catch_unwind(|| rustyzoo::crash::guard(|| panic!("real")));
    assert!(ended.is_err(), "the panic goes on");
    assert_eq!(ASKED.load(Ordering::SeqCst), 1);
    let ok = rustyzoo::crash::guard(|| 7);
    assert_eq!((ok, ASKED.load(Ordering::SeqCst)), (7, 1));
    let _ = std::fs::remove_dir_all(&dir);
}
