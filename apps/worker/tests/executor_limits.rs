use pixivarchive_worker::executors::ExecutionGate;
use std::time::Duration;

#[tokio::test]
async fn execution_gate_limits_shared_workload_concurrency() {
    let gate = ExecutionGate::new(2, None).unwrap();
    let first = gate.enter().await;
    let second = gate.enter().await;

    assert!(
        tokio::time::timeout(Duration::from_millis(40), gate.enter())
            .await
            .is_err()
    );

    drop(first);
    assert!(
        tokio::time::timeout(Duration::from_millis(40), gate.enter())
            .await
            .is_ok()
    );
    drop(second);
}

#[tokio::test]
async fn execution_gate_spaces_rate_limited_starts() {
    let gate = ExecutionGate::new(2, Some((2, Duration::from_millis(100)))).unwrap();
    let first = gate.enter().await;
    drop(first);
    let started = tokio::time::Instant::now();
    let second = gate.enter().await;

    assert!(started.elapsed() >= Duration::from_millis(40));
    drop(second);
}

#[tokio::test]
async fn execution_gate_keeps_rate_spacing_after_concurrency_waits() {
    let gate = ExecutionGate::new(1, Some((10, Duration::from_secs(1)))).unwrap();
    let first = gate.enter().await;
    let second_gate = gate.clone();
    let third_gate = gate.clone();
    let second = tokio::spawn(async move {
        let permit = second_gate.enter().await;
        let started = tokio::time::Instant::now();
        drop(permit);
        started
    });
    let third = tokio::spawn(async move {
        let permit = third_gate.enter().await;
        let started = tokio::time::Instant::now();
        drop(permit);
        started
    });

    tokio::time::sleep(Duration::from_millis(220)).await;
    drop(first);
    let second_started = second.await.unwrap();
    let third_started = third.await.unwrap();

    assert!(
        third_started.duration_since(second_started) >= Duration::from_millis(80),
        "queued work must still respect the configured start interval"
    );
}
