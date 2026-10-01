use super::soft_limit_to_ask;

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
