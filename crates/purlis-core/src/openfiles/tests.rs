use super::{Raised, settle, soft_limit_to_ask};

#[test]
fn a_launchd_started_mac_app_asks_for_open_max_and_not_the_unlimited_hard_limit() {
    // launchd gives a GUI app a soft limit of 256 and an unlimited hard one; asking macOS for
    // RLIM_INFINITY answers EINVAL, so OPEN_MAX (10240, <sys/syslimits.h>) is the ask.
    assert_eq!(soft_limit_to_ask(Some(256), None), Some(10_240));
}

#[test]
fn a_hard_limit_below_open_max_is_asked_for_whole() {
    assert_eq!(soft_limit_to_ask(Some(1_024), Some(4_096)), Some(4_096));
}

#[test]
fn a_hard_limit_above_open_max_is_capped_at_open_max() {
    // systemd's default hard limit.
    assert_eq!(soft_limit_to_ask(Some(1_024), Some(524_288)), Some(10_240));
}

#[test]
fn a_soft_limit_already_past_open_max_is_never_lowered() {
    // A terminal whose shell raised its own limit hands the app 1 048 576.
    assert_eq!(soft_limit_to_ask(Some(1_048_576), None), None);
}

#[test]
fn a_soft_limit_already_at_the_ceiling_is_left_alone() {
    assert_eq!(soft_limit_to_ask(Some(10_240), None), None);
    assert_eq!(soft_limit_to_ask(Some(4_096), Some(4_096)), None);
}

#[test]
fn an_unlimited_soft_limit_is_left_alone() {
    assert_eq!(soft_limit_to_ask(None, None), None);
}

/// A system that accepts any soft limit up to `ceiling`, as macOS's `setrlimit` does up to
/// `kern.maxfilesperproc`, and keeps the last one it accepted.
struct System {
    ceiling: u64,
    limit: u64,
    calls: usize,
}

impl System {
    fn at(limit: u64, ceiling: u64) -> Self {
        Self {
            ceiling,
            limit,
            calls: 0,
        }
    }

    fn accepts(&mut self, asked: u64) -> bool {
        self.calls += 1;
        let accepted = asked <= self.ceiling;
        if accepted {
            self.limit = asked;
        }
        accepted
    }
}

#[test]
fn a_system_that_accepts_the_ask_is_asked_once() {
    let mut system = System::at(256, 1_000_000);
    assert_eq!(
        settle(256, 10_240, |asked| system.accepts(asked)),
        Some(10_240)
    );
    assert_eq!(system.calls, 1);
    assert_eq!(system.limit, 10_240);
}

#[test]
fn a_ceiling_below_the_ask_is_found_and_the_process_ends_at_it() {
    // `kern.maxfilesperproc` set to 5000, by `sysctl` or by a managed profile.
    let mut system = System::at(256, 5_000);
    assert_eq!(
        settle(256, 10_240, |asked| system.accepts(asked)),
        Some(5_000)
    );
    assert_eq!(
        system.limit, 5_000,
        "the last limit set is the one reported"
    );
    assert!(system.calls <= 15, "{} calls", system.calls);
}

#[test]
fn every_ceiling_between_the_start_and_the_ask_is_found_exactly() {
    for ceiling in [257, 258, 1_023, 1_024, 4_097, 10_239] {
        let mut system = System::at(256, ceiling);
        assert_eq!(
            settle(256, 10_240, |asked| system.accepts(asked)),
            Some(ceiling)
        );
        assert_eq!(system.limit, ceiling);
    }
}

#[test]
fn a_ceiling_at_or_below_the_start_settles_nothing() {
    for ceiling in [0, 255, 256] {
        let mut system = System::at(256, ceiling);
        assert_eq!(settle(256, 10_240, |asked| system.accepts(asked)), None);
        assert_eq!(system.limit, 256, "the limit the process was given is kept");
    }
}

#[test]
fn the_launch_log_never_calls_a_platform_without_the_limit_unlimited() {
    assert_eq!(
        Raised::NotApplicable.to_string(),
        "this platform has no open-file limit to raise"
    );
    assert_eq!(
        Raised::AlreadyEnough(None).to_string(),
        "the open-file limit is already unlimited"
    );
}
