use super::*;

async fn attach(running: &Running) -> crate::Terminal {
    running
        .runtime
        .attach(
            &binding(),
            TerminalSize::new(80, 24).unwrap(),
            SystemTime::now() + Duration::from_secs(60),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn empty_history_has_an_explicit_boundary_without_process_output() {
    let running = Running::start().await;
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.ssh.initial_output = Some(vec![]);
    }
    let mut terminal = attach(&running).await;
    assert!(
        tokio::time::timeout(Duration::from_secs(2), terminal.read())
            .await
            .unwrap()
            .unwrap()
            == Some(TerminalOutput::ReplayComplete)
    );
    terminal.detach().await.unwrap();
    wait_for_revoke(&running.fake).await;
}

#[tokio::test]
async fn generated_boundary_is_not_batched_with_history_or_immediate_live_queries() {
    let running = Running::start().await;
    let history = b"history\x1b[6n\x1b]11;?\x07\x00\xff".to_vec();
    let live = b"live\x1b[6n\x1b]11;?\x07\x00\xff".to_vec();
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.ssh.initial_output = Some(history.chunks(2).map(<[u8]>::to_vec).collect());
        state.ssh.live_output = live.chunks(2).map(<[u8]>::to_vec).collect();
    }
    let mut terminal = attach(&running).await;
    let (mut before, mut after, mut boundary) = (vec![], vec![], false);
    tokio::time::timeout(Duration::from_secs(3), async {
        while after.len() < live.len() {
            match terminal.read().await.unwrap().unwrap() {
                TerminalOutput::Data(bytes) if !boundary => before.extend(bytes),
                TerminalOutput::Data(bytes) => after.extend(bytes),
                TerminalOutput::ReplayComplete => {
                    assert!(!boundary);
                    boundary = true;
                }
            }
        }
    })
    .await
    .unwrap();
    assert!(boundary);
    assert_eq!(before, history);
    assert_eq!(after, live);
    terminal.detach().await.unwrap();
}

#[tokio::test]
async fn malformed_duplicate_and_missing_replay_events_fail_and_revoke() {
    for mode in 0..3 {
        let running = Running::start().await;
        {
            let mut state = running.fake.0.lock().unwrap();
            state.sandbox = Some(sandbox(Phase::Ready));
            state.ssh.initial_output = Some(vec![]);
            match mode {
                0 => state.ssh.replay_metadata = Some(vec![10, 255]),
                1 => state.ssh.duplicate_replay = true,
                _ => {
                    state.ssh.omit_replay = true;
                    state.ssh.initial_output_eof = true;
                }
            }
        }
        let mut terminal = attach(&running).await;
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                match terminal.read().await {
                    Err(_) => break,
                    Ok(Some(TerminalOutput::ReplayComplete)) if mode == 1 => {}
                    _ => panic!("invalid replay was accepted"),
                }
            }
        })
        .await
        .unwrap();
        let _ = terminal.detach().await;
        wait_for_revoke(&running.fake).await;
        let state = running.fake.0.lock().unwrap();
        assert_eq!(state.stops, 0);
        assert_eq!(state.ssh.eofs, 0);
    }
}

#[tokio::test]
async fn acknowledged_env_without_metadata_times_out_even_when_terminal_is_not_read() {
    let running = Running::start().await;
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.ssh.initial_output = Some(vec![]);
        state.ssh.omit_replay = true;
    }
    let mut terminal = attach(&running).await;
    tokio::time::sleep(Duration::from_millis(20_200)).await;
    wait_for_revoke(&running.fake).await;
    assert!(terminal.read().await.is_err());
    assert_eq!(running.fake.0.lock().unwrap().stops, 0);
}
