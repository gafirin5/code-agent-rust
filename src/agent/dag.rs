//! Directed Acyclic Graph (DAG) task validation and topological ordering.

pub use crate::agent::tasks::{DagTask, DagValidationError, DagValidator};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_linear_dag() {
        let tasks = vec![
            DagTask::new("A", vec![]),
            DagTask::new("B", vec!["A".into()]),
            DagTask::new("C", vec!["B".into()]),
        ];
        assert!(DagValidator::validate(&tasks).is_ok());
        let sorted = DagValidator::topological_sort(&tasks).unwrap();
        assert_eq!(sorted, vec!["A", "B", "C"]);
    }

    #[test]
    fn test_valid_diamond_dag() {
        let tasks = vec![
            DagTask::new("A", vec![]),
            DagTask::new("B", vec!["A".into()]),
            DagTask::new("C", vec!["A".into()]),
            DagTask::new("D", vec!["B".into(), "C".into()]),
        ];
        assert!(DagValidator::validate(&tasks).is_ok());
        let sorted = DagValidator::topological_sort(&tasks).unwrap();
        assert_eq!(sorted[0], "A");
        assert_eq!(sorted[3], "D");
    }

    #[test]
    fn test_reject_self_cycle() {
        let tasks = vec![DagTask::new("A", vec!["A".into()])];
        let err = DagValidator::validate(&tasks).unwrap_err();
        assert!(matches!(err, DagValidationError::CycleDetected(_)));
    }

    #[test]
    fn test_reject_direct_cycle() {
        let tasks = vec![
            DagTask::new("A", vec!["B".into()]),
            DagTask::new("B", vec!["A".into()]),
        ];
        let err = DagValidator::validate(&tasks).unwrap_err();
        assert!(matches!(err, DagValidationError::CycleDetected(_)));
    }

    #[test]
    fn test_reject_indirect_cycle() {
        let tasks = vec![
            DagTask::new("A", vec!["B".into()]),
            DagTask::new("B", vec!["C".into()]),
            DagTask::new("C", vec!["A".into()]),
        ];
        let err = DagValidator::validate(&tasks).unwrap_err();
        assert!(matches!(err, DagValidationError::CycleDetected(_)));
    }

    #[test]
    fn test_reject_missing_prerequisite() {
        let tasks = vec![DagTask::new("A", vec!["NONEXISTENT".into()])];
        let err = DagValidator::validate(&tasks).unwrap_err();
        assert_eq!(
            err,
            DagValidationError::MissingDependency {
                task: "A".into(),
                missing: "NONEXISTENT".into(),
            }
        );
    }
}
