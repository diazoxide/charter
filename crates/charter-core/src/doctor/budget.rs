//! `forge budget`: each forge account's request budget and its use this hour (FI14, FW-4).
//!
//! One row per account charter has sent requests as, read from what the sending process kept
//! in the machine tier ([`crate::forge::budget::kept`]). **Shown only when there is one**: a
//! machine that has signed in to no forge has nothing to report.

use super::{Doctor, Row};
use crate::forge::budget::{self, Clock, SystemClock};

const NAME: &str = "forge budget";

pub(super) fn budgets(d: &Doctor) -> Vec<Row> {
    let Some(config_root) = &d.budgets else {
        return Vec::new();
    };
    let now = SystemClock.now();
    budget::kept(config_root, now)
        .into_iter()
        .map(|usage| {
            let said = usage.said();
            if usage.spent() {
                Row::warn(
                    NAME,
                    said,
                    "charter's hour for this account is spent, so background refreshes wait \
                     until it turns; what you ask for still goes through.",
                )
            } else if usage.below_floor(now) {
                Row::warn(
                    NAME,
                    said,
                    format!(
                        "Less than {}% of the forge's own limit is left, so charter polls this \
                         account {} times less often until it resets.",
                        budget::FLOOR_PERCENT,
                        crate::forge::poll::BACK_OFF
                    ),
                )
            } else {
                Row::ok(NAME, said)
            }
        })
        .collect()
}
