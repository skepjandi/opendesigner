//! The runner against a real tmux server on a private socket. Skipped where tmux is missing.

use std::time::{Duration, Instant};

use od_code::plan::{RunPlan, Service, ServiceKind};
use od_code::runner::{Runner, State};
use od_code::tmux::{SystemTmux, Tmux};

fn tmux_present() -> bool {
    SystemTmux::with_socket("od-test-probe")
        .run(&["-V".to_string()])
        .is_ok_and(|r| r.success)
}

fn plan(command: &str) -> RunPlan {
    RunPlan {
        services: vec![Service {
            name: "job".into(),
            kind: ServiceKind::Process,
            command: command.into(),
            cwd: ".".into(),
            port: None,
        }],
    }
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn output_and_the_exit_code_of_a_service_stay_readable() {
    if !tmux_present() {
        return;
    }
    let repo = tempfile::tempdir().unwrap();
    let socket = format!("od-test-{}", std::process::id());
    let runner = Runner::new(SystemTmux::with_socket(&socket), repo.path(), &socket);

    let crashing = plan("echo starting up; echo 'boom: cannot bind' >&2; exit 3");
    runner.start(&crashing, repo.path(), "job").unwrap();
    wait_for("the crash", || {
        matches!(
            runner.status(&crashing).unwrap()[0].state,
            State::Exited { code: 3 }
        )
    });
    let logs = runner.logs("job", 50).unwrap();
    assert!(
        logs.contains("starting up") && logs.contains("boom: cannot bind"),
        "{logs}"
    );

    let waiting = plan("echo up; sleep 60");
    runner.start(&waiting, repo.path(), "job").unwrap();
    wait_for("running", || {
        runner.status(&waiting).unwrap()[0].state == State::Running
            && runner.logs("job", 5).unwrap().contains("up")
    });
    assert!(runner.start(&waiting, repo.path(), "job").is_err());

    runner.stop_all(&waiting).unwrap();
    assert_eq!(runner.status(&waiting).unwrap()[0].state, State::Stopped);

    let _ = SystemTmux::with_socket(&socket).run(&["kill-server".to_string()]);
}
