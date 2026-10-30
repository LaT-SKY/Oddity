#[test]
fn controller_activity_renews_expires_clears_and_releases_on_drop() {
    use crate::gamepad::idle::ActivityInhibitor;
    let bus = Bus::new();
    let client = bus.client.clone();
    let guard = ActivityInhibitor::with_request(Duration::from_millis(120), move || {
        IdleInhibitor::start_on(client.clone(), "controller test")
    });
    let wait = |expected: bool| {
        for _ in 0..200 {
            if guard.status().requested == expected {
                return;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("idle state did not change")
    };
    guard.activity();
    wait(true);
    guard.activity();
    std::thread::sleep(Duration::from_millis(20));
    assert!(guard.status().requested);
    wait(false);
    assert!(bus.calls.lock().unwrap().idle_closed > 0);
    guard.activity();
    wait(true);
    guard.clear();
    wait(false);
    guard.activity();
    wait(true);
    drop(guard);
    for _ in 0..200 {
        if bus.calls.lock().unwrap().idle_closed >= 3 {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(bus.calls.lock().unwrap().idle_closed >= 3);
    let failing =
        ActivityInhibitor::with_request(Duration::from_millis(20), || Err("not supported".into()));
    failing.activity();
    for _ in 0..100 {
        if failing.status().error.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(failing.status().error.as_deref(), Some("not supported"));
    failing.clear();
    let immediate = ActivityInhibitor::with_request(Duration::ZERO, || {
        panic!("expired event must not request")
    });
    immediate.activity();
    immediate.clear();
    drop(immediate);
}
