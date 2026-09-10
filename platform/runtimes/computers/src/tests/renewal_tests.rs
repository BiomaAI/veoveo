use super::*;
use tokio::time::Instant;

#[tokio::test]
async fn renewing_blocked_output_keeps_one_attachment_and_revoke_closes_it() {
    let running = Running::start().await;
    banner_fixture(&running, 16);
    let (authority, lease) = LeaseAuthority::issue(Instant::now(), Duration::from_secs(1)).unwrap();
    let mut terminal = running
        .runtime
        .attach(&binding(), TerminalSize::new(100, 30).unwrap(), lease)
        .await
        .unwrap();
    for _ in 0..5 {
        tokio::time::sleep(Duration::from_millis(250)).await;
        authority
            .renew(Instant::now(), Duration::from_secs(1))
            .unwrap();
    }
    // Output remains full while renewal and input continue beyond the first lease.
    terminal.write(b"still-attached").await.unwrap();
    assert_eq!(
        running.fake.0.lock().unwrap().ssh.subsystems,
        ["openshell-main"]
    );
    let started = Instant::now();
    authority.revoke();
    wait_for_revoke(&running.fake).await;
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(matches!(
        terminal.read().await,
        Err(RuntimeFailure::LeaseExpired)
    ));
    assert!(matches!(
        terminal.write(b"forbidden").await,
        Err(RuntimeFailure::LeaseExpired)
    ));
    assert!(
        authority
            .renew(Instant::now(), Duration::from_secs(30))
            .is_err()
    );
    let state = running.fake.0.lock().unwrap();
    assert_eq!(state.ssh.eofs, 0);
    assert_eq!(state.stops, 0);
}

#[tokio::test]
async fn revoked_attachment_unblocks_a_writer_stalled_at_the_peer() {
    let running = Running::start().await;
    let gate = Gate::default();
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.ssh.data_gate = Some(gate.clone());
    }
    let (authority, lease) =
        LeaseAuthority::issue(Instant::now(), Duration::from_secs(30)).unwrap();
    let terminal = running
        .runtime
        .attach(&binding(), TerminalSize::new(100, 30).unwrap(), lease)
        .await
        .unwrap();
    let writer = tokio::spawn(async move {
        for _ in 0..256 {
            if terminal.write(&vec![1; MAX_CHUNK_BYTES]).await.is_err() {
                return;
            }
        }
        panic!("fixture failed to backpressure the writer");
    });
    gate.wait().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !writer.is_finished(),
        "writer must be stalled before revocation"
    );
    authority.revoke();
    tokio::time::timeout(Duration::from_secs(3), writer)
        .await
        .unwrap()
        .unwrap();
    wait_for_revoke(&running.fake).await;
    gate.release.notify_one();
    assert_eq!(running.fake.0.lock().unwrap().stops, 0);
}

#[tokio::test]
async fn authority_loss_during_a_mint_revokes_the_late_response() {
    let running = Running::start().await;
    let gate = Gate::default();
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.session_reply_gate = Some(gate.clone());
    }
    let (authority, lease) =
        LeaseAuthority::issue(Instant::now(), Duration::from_secs(30)).unwrap();
    let access = running
        .runtime
        .open_shell_access(&binding(), lease)
        .await
        .unwrap();
    let request = tokio::spawn(async move { access.create_ssh_session("sandbox-1").await });
    gate.wait().await;
    drop(authority);
    assert!(matches!(
        tokio::time::timeout(Duration::from_secs(1), request)
            .await
            .unwrap()
            .unwrap(),
        Err(RuntimeFailure::LeaseExpired)
    ));
    gate.release.notify_one();
    wait_for_revoke(&running.fake).await;
}
