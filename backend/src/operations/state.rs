use super::contracts::JobState;

pub fn can_transition(from: JobState, to: JobState) -> bool {
    matches!(
        (from, to),
        (JobState::AwaitingApproval, JobState::Queued)
            | (JobState::AwaitingApproval, JobState::Rejected)
            | (JobState::AwaitingApproval, JobState::Expired)
            | (JobState::Queued, JobState::Running)
            | (JobState::Queued, JobState::Cancelled)
            | (JobState::Running, JobState::Succeeded)
            | (JobState::Running, JobState::Failed)
            | (JobState::Running, JobState::Cancelled)
            | (JobState::Running, JobState::NeedsAttention)
            | (JobState::NeedsAttention, JobState::Succeeded)
            | (JobState::NeedsAttention, JobState::Failed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_state_machine_allows_only_designed_transitions() {
        let states = [
            JobState::AwaitingApproval,
            JobState::Queued,
            JobState::Running,
            JobState::Succeeded,
            JobState::Failed,
            JobState::Cancelled,
            JobState::NeedsAttention,
            JobState::Rejected,
            JobState::Expired,
        ];
        let allowed = states
            .into_iter()
            .flat_map(|from| states.into_iter().map(move |to| (from, to)))
            .filter(|(from, to)| can_transition(*from, *to))
            .count();
        assert_eq!(allowed, 11);
        assert!(!can_transition(JobState::NeedsAttention, JobState::Queued));
        assert!(!can_transition(JobState::Succeeded, JobState::Running));
    }
}
