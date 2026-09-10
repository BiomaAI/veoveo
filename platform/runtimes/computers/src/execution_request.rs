//! Only a fixed guest launcher reaches the provider's command-preview logs.
use std::{future::Future, io::Cursor};

use veoveo_computer_execution::{ExecutionRequest, LAUNCHER_PATH, RETAINED_HOME};

use crate::{
    Binding, ExecChunk, ExecInput, ExecIntent, ExecResult, OpenShellRuntime, Result, RuntimeFailure,
};

impl OpenShellRuntime {
    /// Requires caller-owned current authority, a durable execution fence and a
    /// template qualified with the matching guest launcher. Observation loss is
    /// uncertain; this method does not cancel a process or settle the domain.
    pub async fn execute_request<F, Fut>(
        &self,
        binding: &Binding,
        request: &ExecutionRequest,
        timeout_seconds: u32,
        maximum_output_bytes: usize,
        on_output: F,
    ) -> Result<ExecResult>
    where
        F: FnMut(ExecChunk) -> Fut + Send,
        Fut: Future<Output = Result<()>> + Send,
    {
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
        self.execute_with_input(
            binding,
            &intent,
            ExecInput::new(&mut input, byte_count)?,
            on_output,
        )
        .await
    }
}
