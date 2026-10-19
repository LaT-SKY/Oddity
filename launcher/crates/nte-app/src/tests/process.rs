use super::EnvGuard;
use crate::*;
use std::time::Instant;

#[test]
fn now_millis_is_a_wall_clock_timestamp() {
    assert!(now_millis() > 1_600_000_000_000);
}

#[test]
fn game_status_reflects_the_session() {
    let idle = GameStatus::idle();
    assert!(!idle.running);
    assert!(!idle.closing);
    assert!(idle.pid.is_none());
    assert!(idle.started_at.is_none());
    assert!(idle.exe.is_none());
    assert!(idle.log.is_none());

    assert!(!GameStatus::from_session(None).running);

    let session = GameSession {
        id: 7,
        pid: 1234,
        pgid: 1234,
        exe: "/game/HTGame.exe".to_string(),
        started_at: 99,
        log: "/tmp/launch.log".to_string(),
        closing: true,
    };
    let status = GameStatus::from_session(Some(&session));
    assert!(status.running);
    assert!(status.closing);
    assert_eq!(status.pid, Some(1234));
    assert_eq!(status.started_at, Some(99));
    assert_eq!(status.exe.as_deref(), Some("/game/HTGame.exe"));
    assert_eq!(status.log.as_deref(), Some("/tmp/launch.log"));
}

#[test]
fn rejects_non_process_groups() {
    assert!(!process_group_alive(0));
    assert!(!process_group_alive(-1));
    assert_eq!(signal_process_group(0, libc::SIGTERM), Ok(()));
    assert_eq!(signal_process_group(-1, libc::SIGTERM), Ok(()));
    terminate_process_group(0).expect("nothing to terminate");
}

#[test]
fn signals_and_terminates_a_live_process_group() {
    // PATH 会被别的用例改写，启动外部命令前先拿到环境锁
    let _env = EnvGuard::new();
    let mut child = Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("spawn sleep");
    let pgid = i32::try_from(child.id()).expect("pid fits i32");
    assert!(process_group_alive(pgid));

    signal_process_group(pgid, libc::SIGTERM).expect("signal group");
    let _ = child.wait();

    // 组长被回收后进程组也不该再被认为存活
    let deadline = Instant::now() + Duration::from_secs(2);
    while process_group_alive(pgid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!process_group_alive(pgid));
    // 已经消失的进程组再发信号仍然算成功
    signal_process_group(pgid, libc::SIGTERM).expect("signal dead group");
    terminate_process_group(pgid).expect("terminate dead group");
}

#[test]
fn terminates_a_process_group_and_lets_the_leader_reap() {
    // PATH 会被别的用例改写，启动外部命令前先拿到环境锁
    let _env = EnvGuard::new();
    let mut child = Command::new("sleep")
        .arg("30")
        .process_group(0)
        .spawn()
        .expect("spawn sleep");
    let pgid = i32::try_from(child.id()).expect("pid fits i32");
    assert!(process_group_alive(pgid));

    // 应用里由监控线程回收组长；没有并发的 wait，组长会一直是僵尸进程，
    // 整个进程组也就永远不会消失。
    let waiter = thread::spawn(move || child.wait());
    terminate_process_group(pgid).expect("terminate group");
    let _ = waiter.join().expect("join waiter").expect("wait for child");

    let deadline = Instant::now() + Duration::from_secs(2);
    while process_group_alive(pgid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!process_group_alive(pgid));
}

#[test]
fn terminates_a_group_with_members_beyond_the_leader() {
    // PATH 会被别的用例改写，启动外部命令前先拿到环境锁
    let _env = EnvGuard::new();
    // 用 shell 起一个后台子进程：SIGTERM 只发给组长时组内还有成员
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("sleep 30 & wait")
        .process_group(0)
        .spawn()
        .expect("spawn shell");
    let pgid = i32::try_from(child.id()).expect("pid fits i32");
    let waiter = thread::spawn(move || child.wait());
    terminate_process_group(pgid).expect("terminate group");
    let _ = waiter.join().expect("join waiter");

    let deadline = Instant::now() + Duration::from_secs(2);
    while process_group_alive(pgid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!process_group_alive(pgid));
}
