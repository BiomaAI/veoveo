//! Only a fixed guest launcher reaches the provider's command-preview logs.
use std::{future::Future, io::Cursor};

use veoveo_computer_execution::{ExecutionRequest, LAUNCHER_PATH, RETAINED_HOME};

use crate::{
    Binding, ExecChunk, ExecInput, ExecIntent, ExecResult, Observation, OpenShellRuntime, Phase,
    Result, RuntimeFailure,
};

impl OpenShellRuntime {
    /// Requires caller-owned current authority, a durable execution fence and a
    /// template qualified with the matching guest launcher. Observation loss is
    /// uncertain; this method does not cancel a process or settle the domain.
    pub async fn execute_request<F, Fut>(
        &self,
        binding: &Binding,
        expected: &Observation,
        request: &ExecutionRequest,
        timeout_seconds: u32,
        maximum_output_bytes: usize,
        on_output: F,
    ) -> Result<ExecResult>
    where
        F: FnMut(ExecChunk) -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send,
    {
        if expected.phase != Phase::Ready
            || expected.exit_code.is_some()
            || !crate::models::identifier(&expected.sandbox_id)
            || !crate::models::identifier(&expected.main_process_instance_id)
        {
            return Err(RuntimeFailure::BindingMismatch);
        }
        let frame = request
            .encode()
            .map_err(|_| RuntimeFailure::InvalidExecution)?;
        let byte_count = frame.len() as u64;
        let mut input = Cursor::new(frame);
        let intent = ExecIntent::new(
            vec![LAUNCHER_PATH.into()],
            RETAINED_HOME.into(),
            timeout_seconds,
            maximum_output_bytes,
            vec![],
        )?;
        self.execute_inner(
            binding,
            &intent,
            on_output,
            Some(ExecInput::new(&mut input, byte_count)?),
            Some(expected),
        )
        .await
    }
}
