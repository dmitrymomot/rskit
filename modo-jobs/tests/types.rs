use modo_jobs::{JobId, JobState};
use std::str::FromStr;

#[test]
fn test_job_id_unique() {
    let id1 = JobId::new();
    let id2 = JobId::new();
    assert_ne!(id1, id2);
}

#[test]
fn test_job_id_is_26_char_ulid() {
    let id = JobId::new();
    assert_eq!(id.as_str().len(), 26);
}

#[test]
fn test_job_id_display() {
    let id = JobId::new();
    assert_eq!(id.to_string(), id.as_str());
}

#[test]
fn test_job_state_display_roundtrip() {
    let states = [
        JobState::Pending,
        JobState::Running,
        JobState::Completed,
        JobState::Dead,
        JobState::Cancelled,
    ];
    for state in states {
        let s = state.to_string();
        let parsed = JobState::from_str(&s).unwrap();
        assert_eq!(parsed, state);
    }
}

#[test]
fn test_job_state_from_str_invalid() {
    let result = JobState::from_str("unknown");
    assert!(result.is_err());
}
