use super::*;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::sync::{Arc, Mutex};

#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum Observation<'a> {
    CreateIntent {
        request: &'a veoveo_time_mcp::contract::CreateTemporalEventRequest,
    },
    CancelIntent {
        request: &'a veoveo_time_mcp::contract::CancelTemporalEventRequest,
    },
    Event {
        phase: &'a str,
        event: &'a veoveo_time_mcp::contract::TemporalEvent,
    },
    ReplacementIntent {
        component: &'a str,
    },
    Wakes {
        phase: &'a str,
        wakes: &'a [veoveo_agent_runtime::persistence::WakeRecord],
    },
    Passed {
        effects: usize,
    },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct RetainedEvent {
    pub phase: String,
    pub event: veoveo_time_mcp::contract::TemporalEvent,
}
struct State {
    file: fs::File,
    root: PathBuf,
    events: Vec<RetainedEvent>,
    first_failure: Option<String>,
    preserved: usize,
}
/// This owner survives cancellation of the scenario waiter. A failed append
/// cannot erase an already received typed identity or authorize another dispatch.
#[derive(Clone)]
pub(super) struct Journal(Arc<Mutex<State>>);
impl Journal {
    pub fn new(file: fs::File, root: PathBuf) -> Self {
        Self(Arc::new(Mutex::new(State {
            file,
            root,
            events: Vec::new(),
            first_failure: None,
            preserved: 0,
        })))
    }
    pub fn append(&self, observation: Observation<'_>) -> Result<()> {
        let mut state = self.0.lock().expect("event journal owner");
        if let Observation::Event { phase, event } = &observation {
            anyhow::ensure!(
                state.events.len() < 16,
                "event fixture observation bound exhausted"
            );
            state.events.push(RetainedEvent {
                phase: (*phase).to_owned(),
                event: (*event).clone(),
            });
        }
        if let Some(failure) = &state.first_failure {
            anyhow::bail!("original event journal persistence failed: {failure}");
        }
        let result = (|| -> Result<()> {
            serde_json::to_writer(&mut state.file, &observation)?;
            state.file.write_all(b"\n")?;
            state.file.sync_all()?;
            Ok(())
        })();
        if let Err(error) = result {
            state.first_failure = Some(error.to_string());
            return Err(error);
        }
        Ok(())
    }
    pub fn flush(&self) -> Result<()> {
        let mut state = self.0.lock().expect("event journal owner");
        if state.first_failure.is_some() && state.preserved < state.events.len() {
            // Never append after an uncertain partial write or overwrite original
            // evidence. A create-new snapshot preserves all received typed facts.
            let path = state
                .root
                .join(format!("retained-events-{}.json", uuid::Uuid::now_v7()));
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(path)?;
            serde_json::to_writer(&mut file, &state.events)?;
            file.write_all(b"\n")?;
            file.sync_all()?;
            state.preserved = state.events.len();
        }
        if let Some(failure) = &state.first_failure {
            anyhow::bail!("original event journal persistence failed: {failure}");
        }
        if let Err(error) = state.file.sync_all() {
            state.first_failure = Some(error.to_string());
            return Err(error.into());
        }
        Ok(())
    }
    #[cfg(test)]
    pub fn events(&self) -> Vec<RetainedEvent> {
        self.0.lock().unwrap().events.clone()
    }
}
