use pixivarchive_domain::subscription::{SubscriptionSchedule, SubscriptionScheduleError};
use time::{Date, Duration, OffsetDateTime, Time};

fn schedule(interval_minutes: i64) -> SubscriptionSchedule {
    SubscriptionSchedule::new(interval_minutes, 2).unwrap()
}

#[test]
fn future_schedule_keeps_its_original_anchor() {
    let now = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
    let scheduled_for = now + Duration::minutes(30);

    assert_eq!(
        schedule(60).next_run_after(scheduled_for, now).unwrap(),
        scheduled_for
    );
}

#[test]
fn due_schedule_advances_to_the_first_occurrence_after_now() {
    let scheduled_for = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();

    assert_eq!(
        schedule(60)
            .next_run_after(scheduled_for, scheduled_for)
            .unwrap(),
        scheduled_for + Duration::hours(1)
    );
}

#[test]
fn delayed_schedule_skips_missed_occurrences_without_drifting() {
    let now = OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap();
    let scheduled_for = now - Duration::minutes(130);

    assert_eq!(
        schedule(60).next_run_after(scheduled_for, now).unwrap(),
        scheduled_for + Duration::minutes(180)
    );
}

#[test]
fn next_occurrence_outside_the_supported_time_range_is_rejected() {
    let scheduled_for = Date::MAX.with_time(Time::MAX).assume_utc() - Duration::minutes(1);

    assert_eq!(
        schedule(15).next_run_after(scheduled_for, scheduled_for),
        Err(SubscriptionScheduleError::OutOfRange)
    );
}
