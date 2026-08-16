use crate::settings::SecuritySettings;
use pixivarchive_db::auth::{RateLimitKind, RateLimitReservation};
use time::Duration;
use uuid::Uuid;

pub fn login_reservations(
    source_bucket: &str,
    settings: &SecuritySettings,
) -> Vec<RateLimitReservation> {
    vec![
        reservation(
            RateLimitKind::Entry,
            &format!("entry:{source_bucket}"),
            settings.entry_source_failures.threshold as i32,
            settings.entry_source_failures.window(),
            settings.entry_source_failures.cooldown(),
        ),
        reservation(
            RateLimitKind::Password,
            "password:admin",
            settings.password_failures.threshold as i32,
            settings.password_failures.window(),
            settings.password_failures.cooldown(),
        ),
        reservation(
            RateLimitKind::Shared,
            "shared:admin",
            settings.shared_account_failures.threshold as i32,
            settings.shared_account_failures.window(),
            settings.shared_account_failures.cooldown(),
        ),
    ]
}

pub fn password_failure_kinds() -> [RateLimitKind; 3] {
    [
        RateLimitKind::Password,
        RateLimitKind::Shared,
        RateLimitKind::Entry,
    ]
}

fn reservation(
    kind: RateLimitKind,
    key: &str,
    threshold: i32,
    window: Duration,
    cooldown: Duration,
) -> RateLimitReservation {
    RateLimitReservation {
        id: Uuid::now_v7(),
        kind,
        bucket_key: key.to_owned(),
        threshold,
        window,
        cooldown,
        lease: Duration::minutes(5),
    }
}
