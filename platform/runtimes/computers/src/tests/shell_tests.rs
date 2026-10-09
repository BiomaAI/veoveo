//! Fresh-shell attachment controls; stock SSH provides no retained history.
use super::*;

async fn attach(running: &Running) -> crate::Terminal {
    running
        .runtime
        .attach(
            &binding(),
            TerminalSize::new(80, 24).unwrap(),
            running.lease(Duration::from_secs(30)),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn quiet_shell_acceptance_does_not_require_output_or_replay_metadata() {
    let running = Running::start().await;
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.ssh.initial_output = Some(vec![]);
    }
    let mut terminal = attach(&running).await;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), terminal.read())
            .await
            .is_err()
    );
    terminal.detach().await.unwrap();
    wait_for_revoke(&running.fake).await;
    assert_eq!(running.fake.0.lock().unwrap().ssh.shells, 1);
}

#[tokio::test]
async fn fresh_shell_bytes_preserve_terminal_queries_and_binary_output() {
    let running = Running::start().await;
    let output = b"fresh\x1b[6n\x1b]11;?\x07\x00\xff".to_vec();
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.ssh.initial_output = Some(output.chunks(2).map(<[u8]>::to_vec).collect());
    }
    let mut terminal = attach(&running).await;
    let mut received = vec![];
    tokio::time::timeout(Duration::from_secs(3), async {
        while received.len() < output.len() {
            let TerminalOutput::Data(bytes) = terminal.read().await.unwrap().unwrap();
            received.extend(bytes);
        }
    })
    .await
    .unwrap();
    assert_eq!(received, output);
    terminal.detach().await.unwrap();
    let terminal = attach(&running).await;
    assert_eq!(running.fake.0.lock().unwrap().ssh.shells, 2);
    terminal.detach().await.unwrap();
}

#[tokio::test]
async fn accepted_shell_early_eof_is_visible_and_releases_session_without_stopping_computer() {
    let running = Running::start().await;
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.ssh.initial_output = Some(vec![]);
        state.ssh.initial_output_eof = true;
    }
    let mut terminal = attach(&running).await;
    assert!(
        tokio::time::timeout(Duration::from_secs(3), terminal.read())
            .await
            .unwrap()
            .unwrap()
            .is_none()
    );
    terminal.detach().await.unwrap();
    wait_for_revoke(&running.fake).await;
    assert_eq!(running.fake.0.lock().unwrap().stops, 0);
}

#[tokio::test]
async fn shell_eof_before_nonzero_exit_status_reports_failure() {
    let running = Running::start().await;
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.ssh.initial_output = Some(vec![]);
        state.ssh.initial_output_eof = true;
        state.ssh.initial_exit_status = 23;
    }
    let mut terminal = attach(&running).await;
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(3), terminal.read())
            .await
            .unwrap(),
        Err(RuntimeFailure::TerminalFailed)
    ));
    assert!(terminal.detach().await.is_err());
    wait_for_revoke(&running.fake).await;
    assert_eq!(running.fake.0.lock().unwrap().stops, 0);
}
