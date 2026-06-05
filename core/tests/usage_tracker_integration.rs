use shiotsuchi_core::usage_tracker::UsageTracker;
use tempfile::TempDir;

#[test]
fn test_full_lifecycle() {
    let tmp = TempDir::new().unwrap();
    let tracker = UsageTracker::new(tmp.path(), true, Some(3));

    // Use up all 3
    tracker.check_and_increment().unwrap();
    tracker.check_and_increment().unwrap();
    tracker.check_and_increment().unwrap();

    // 4th should fail
    let err = tracker.check_and_increment().unwrap_err();
    let err_msg = format!("{}", err);
    assert!(err_msg.contains("limit") || err_msg.contains("上限"), "Error should mention limit: {}", err_msg);

    // Reset and try again
    tracker.reset().unwrap();
    tracker.check_and_increment().unwrap();
    let (_, count, _) = tracker.current_usage().unwrap();
    assert_eq!(count, 1);
}

#[test]
fn test_disabled_tracker_never_fails() {
    let tmp = TempDir::new().unwrap();
    let tracker = UsageTracker::new(tmp.path(), false, Some(0));
    for _ in 0..10000 {
        tracker.check_and_increment().unwrap();
    }
}
