//! Deterministic lockstep command scheduling primitives.

use crate::sim::command::{Command, CommandEnvelope};

/// Minimal lockstep scheduler state.
#[derive(Debug, Clone, Copy)]
pub struct LockstepScheduler {
    /// Commands are scheduled `now_tick + input_delay_ticks`.
    pub input_delay_ticks: u64,
}

impl LockstepScheduler {
    pub fn new(input_delay_ticks: u64) -> Self {
        Self {
            input_delay_ticks: input_delay_ticks.max(1),
        }
    }

    /// Wrap a command with deterministic execute tick.
    pub fn schedule(
        &self,
        now_tick: u64,
        owner: crate::sim::intern::InternedId,
        payload: Command,
    ) -> CommandEnvelope {
        let execute_tick = now_tick.saturating_add(self.input_delay_ticks);
        CommandEnvelope::new(owner, execute_tick, payload)
    }
}

#[cfg(test)]
mod tests {
    use super::LockstepScheduler;
    use crate::sim::command::Command;
    use crate::sim::intern::test_intern;

    #[test]
    fn schedules_with_input_delay() {
        let sched = LockstepScheduler::new(3);
        let cmd = sched.schedule(10, test_intern("P1"), Command::Stop { entity_id: 99 });
        assert_eq!(cmd.execute_tick, 13);
        assert_eq!(cmd.owner, test_intern("P1"));
    }
}
