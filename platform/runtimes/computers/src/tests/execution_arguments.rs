use super::*;

fn intent(arguments: Vec<String>) -> Result<ExecIntent> {
    ExecIntent::new(arguments, PERSISTENT_HOME.into(), 5, 1024, vec![])
}

#[test]
fn argument_count_and_bytes_are_bounded_without_rejecting_empty_values() {
    assert!(intent(vec!["/usr/bin/printf".into(), "%s".into(), String::new()]).is_ok());
    assert!(intent(vec![String::new(), "argument".into()]).is_err());
    assert!(intent(vec!["/usr/bin/printf".into(), "embedded\0null".into()]).is_err());
    let mut arguments = vec![String::new(); 1024];
    arguments[0] = "/usr/bin/printf".into();
    assert!(intent(arguments.clone()).is_ok());
    arguments.push(String::new());
    assert!(intent(arguments).is_err());

    let program = "/usr/bin/printf".to_owned();
    let remaining = 32768 - program.len();
    assert!(intent(vec![program.clone(), "x".repeat(remaining)]).is_ok());
    assert!(intent(vec![program, "x".repeat(remaining + 1)]).is_err());
}

#[tokio::test]
async fn provider_rpc_preserves_empty_values_quotes_newlines_and_many_arguments() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().sandbox = Some(sandbox(Phase::Ready));
    let mut arguments = vec![
        "/usr/bin/printf".to_owned(),
        String::new(),
        "a 'quoted' value\nwith Unicode: á".to_owned(),
    ];
    arguments.resize(1024, String::new());
    let input = intent(arguments.clone()).unwrap();
    let result = running
        .runtime
        .execute(&binding(), &input, |_| async { Ok(()) })
        .await
        .unwrap();
    assert_eq!(result.exit_code, 7);
    assert_eq!(
        running.fake.0.lock().unwrap().execution_arguments,
        arguments
    );
}

#[tokio::test]
async fn structured_request_values_travel_only_in_framed_stdin() {
    use std::collections::BTreeMap;
    use veoveo_computer_execution::{ExecutionRequest, LAUNCHER_PATH, RETAINED_HOME};

    let running = Running::start().await;
    let request = ExecutionRequest::new(
        vec![
            "python3".into(),
            "private-argument-fixture".into(),
            String::new(),
        ],
        "private-directory-fixture".into(),
        BTreeMap::from([("FIXTURE_VALUE".into(), "private-value\nlast".into())]),
        vec![255; MAX_CHUNK_BYTES + 1],
    )
    .unwrap();
    let frame = request.encode().unwrap();
    {
        let mut state = running.fake.0.lock().unwrap();
        state.sandbox = Some(sandbox(Phase::Ready));
        state.expected_input_bytes = Some(frame.len());
    }
    let expected = running.runtime.get(&binding()).await.unwrap().unwrap();
    let result = running
        .runtime
        .execute_request(&binding(), &expected, &request, 5, 1024, |_| async {
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(result.exit_code, 0);
    let state = running.fake.0.lock().unwrap();
    let start = state.execution_start.as_ref().unwrap();
    assert_eq!(start.command, [LAUNCHER_PATH]);
    assert_eq!(start.workdir, RETAINED_HOME);
    assert!(start.environment.is_empty());
    assert!(start.stdin.is_empty());
    assert!(!start.tty);
    assert_eq!(state.input_bytes, frame.as_slice());
}

#[tokio::test]
async fn structured_execution_refuses_another_resource_run_or_stopped_expectation_before_start() {
    let running = Running::start().await;
    running.fake.0.lock().unwrap().sandbox = Some(sandbox(Phase::Ready));
    let expected = running.runtime.get(&binding()).await.unwrap().unwrap();
    let request = veoveo_computer_execution::ExecutionRequest::new(
        vec!["/bin/echo".into(), "must-not-start".into()],
        ".".into(),
        Default::default(),
        vec![],
    )
    .unwrap();
    let mut wrong_resource = expected.clone();
    wrong_resource.sandbox_id = "different-resource".into();
    let mut wrong_run = expected.clone();
    wrong_run.main_process_instance_id = "different-process".into();
    let mut stopped = expected.clone();
    stopped.phase = Phase::Stopped;
    let mut exited = expected.clone();
    exited.exit_code = Some(0);
    for before in [wrong_resource, wrong_run, stopped, exited] {
        assert_eq!(
            running
                .runtime
                .execute_request(&binding(), &before, &request, 5, 1024, |_| async { Ok(()) })
                .await,
            Err(RuntimeFailure::BindingMismatch)
        );
    }
    {
        let mut state = running.fake.0.lock().unwrap();
        state
            .sandbox
            .as_mut()
            .unwrap()
            .status
            .as_mut()
            .unwrap()
            .main_process_instance_id = "restarted-process".into();
    }
    assert_eq!(
        running
            .runtime
            .execute_request(&binding(), &expected, &request, 5, 1024, |_| async {
                Ok(())
            })
            .await,
        Err(RuntimeFailure::BindingMismatch)
    );
    let state = running.fake.0.lock().unwrap();
    assert!(state.execution_start.is_none());
    assert!(state.input_bytes.is_empty());
}

#[tokio::test]
async fn gateway_requires_its_exact_active_workspace_before_runtime_admission() {
    let running = Running::start().await;
    let original = running.fake.0.lock().unwrap().workspace.clone().unwrap();
    let mut wrong_name = original.clone();
    wrong_name.metadata.as_mut().unwrap().name = "another".into();
    let mut terminating = original.clone();
    terminating.status.as_mut().unwrap().phase =
        crate::protocol::datamodel::v1::WorkspacePhase::Terminating as i32;
    let mut missing_status = original.clone();
    missing_status.status = None;
    for workspace in [
        None,
        Some(wrong_name),
        Some(terminating),
        Some(missing_status),
    ] {
        running.fake.0.lock().unwrap().workspace = workspace;
        assert_eq!(
            running.runtime.ready().await,
            Err(RuntimeFailure::InvalidConfiguration)
        );
    }
    running.fake.0.lock().unwrap().workspace = Some(original);
    running.runtime.ready().await.unwrap();
    let state = running.fake.0.lock().unwrap();
    assert_eq!((state.creates, state.starts, state.stops), (0, 0, 0));
}
