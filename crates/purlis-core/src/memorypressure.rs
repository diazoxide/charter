//! Whether this machine is short on memory: the one reader a dispatch's last step asks
//! (#1467, spec #1434 decision 9, "new starts wait while the machine is short on memory").
//!
//! **The operating system's own verdict, where it gives one**, so no threshold here needs a
//! measurement on a machine under pressure:
//!
//! - **macOS**: the kernel's memory pressure level, `kern.memorystatus_vm_pressure_level`,
//!   read by running `/usr/sbin/sysctl -n` (a `sysctlbyname` call would need `unsafe`). It is
//!   `1` while memory is normal, `2` while the system warns and `4` when it is critical. Only
//!   critical is short (D-1467-1): a Mac sits at the warning level for hours under ordinary
//!   heavy use while memory is still being compressed, and holding every dispatch for that
//!   long would hold them for nothing; critical is where the system starts ending programs to
//!   free memory, and one more chat makes that worse.
//! - **Linux**: the kernel's pressure stall information, `/proc/pressure/memory`. Short where
//!   every task that wanted to run was stalled on memory for at least a tenth of the last ten
//!   seconds (`full avg10` at or over [`PSI_FULL_SHORT`], D-1467-2). A kernel without it is
//!   read from `/proc/meminfo`: short where the memory available is under
//!   [`AVAILABLE_FLOOR`] and under [`AVAILABLE_SHARE`] of the whole, the smaller of the two.
//! - **Anything else, or a read that fails**, is [`Memory::Unread`], which holds nothing
//!   (D-1467-3): this is a courtesy to the machine and not a boundary, and a reader that
//!   cannot answer must not stop every dispatch.
//!
//! **A seam a test sets** ([`Gauge::stand_in`]): the app holds one [`Gauge`] per project, and a
//! test puts a reading in place of the machine's without touching the machine.

use std::sync::{Mutex, PoisonError};

/// What this machine's memory is, for a start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Memory {
    /// Short: a new chat waits.
    Short,
    /// Not short.
    Enough,
    /// The machine could not be read, or purlis has no reader for it here. Holds nothing.
    Unread,
}

impl Memory {
    /// Whether a start waits on it.
    pub fn is_short(self) -> bool {
        self == Self::Short
    }
}

/// The pressure stall share, in percent of the last ten seconds, at or over which Linux is
/// short (`full avg10`).
pub const PSI_FULL_SHORT: f64 = 10.0;

/// The memory available, in bytes, under which a Linux without pressure stall information is
/// short, where that is less than [`AVAILABLE_SHARE`] of the whole.
pub const AVAILABLE_FLOOR: u64 = 512 * 1024 * 1024;

/// The share of the whole memory, in percent, under which a Linux without pressure stall
/// information is short, where that is less than [`AVAILABLE_FLOOR`].
pub const AVAILABLE_SHARE: u64 = 5;

/// This machine's memory now, as its operating system says it.
pub fn read() -> Memory {
    #[cfg(target_os = "macos")]
    {
        macos()
    }
    #[cfg(target_os = "linux")]
    {
        linux()
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Memory::Unread
    }
}

#[cfg(target_os = "macos")]
fn macos() -> Memory {
    let ran = crate::forklock::output(
        std::process::Command::new("/usr/sbin/sysctl")
            .args(["-n", "kern.memorystatus_vm_pressure_level"]),
    );
    match ran {
        Ok(out) if out.status.success() => macos_level(&String::from_utf8_lossy(&out.stdout)),
        _ => Memory::Unread,
    }
}

#[cfg(target_os = "linux")]
fn linux() -> Memory {
    match std::fs::read_to_string("/proc/pressure/memory") {
        Ok(text) => psi(&text),
        Err(_) => {
            std::fs::read_to_string("/proc/meminfo").map_or(Memory::Unread, |text| meminfo(&text))
        }
    }
}

/// What `sysctl -n kern.memorystatus_vm_pressure_level` printed: `4` (critical) or over is
/// short, `1` and `2` are not, anything else is unread.
pub fn macos_level(said: &str) -> Memory {
    match said.trim().parse::<u32>() {
        Ok(level) if level >= 4 => Memory::Short,
        Ok(1..=3) => Memory::Enough,
        _ => Memory::Unread,
    }
}

/// What `/proc/pressure/memory` holds: short where its `full` line's `avg10` is at or over
/// [`PSI_FULL_SHORT`]. A file with no such line is unread.
pub fn psi(text: &str) -> Memory {
    let full = text
        .lines()
        .find_map(|line| line.strip_prefix("full "))
        .and_then(|line| {
            line.split_whitespace()
                .find_map(|field| field.strip_prefix("avg10="))
        })
        .and_then(|avg| avg.parse::<f64>().ok());
    match full {
        Some(avg) if avg >= PSI_FULL_SHORT => Memory::Short,
        Some(_) => Memory::Enough,
        None => Memory::Unread,
    }
}

/// What `/proc/meminfo` holds: short where `MemAvailable` is under the smaller of
/// [`AVAILABLE_FLOOR`] and [`AVAILABLE_SHARE`] of `MemTotal`. A file without both is unread.
pub fn meminfo(text: &str) -> Memory {
    let kib = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key)?.strip_prefix(':'))
            .and_then(|rest| rest.split_whitespace().next())
            .and_then(|n| n.parse::<u64>().ok())
            .map(|n| n.saturating_mul(1024))
    };
    let (Some(total), Some(available)) = (kib("MemTotal"), kib("MemAvailable")) else {
        return Memory::Unread;
    };
    let floor = AVAILABLE_FLOOR.min(total / 100 * AVAILABLE_SHARE);
    if available < floor {
        Memory::Short
    } else {
        Memory::Enough
    }
}

/// **The reader the app asks**, with the seam a test sets: a reading put in place of the
/// machine's ([`Self::stand_in`]) answers until it is taken away.
#[derive(Debug, Default)]
pub struct Gauge {
    stand_in: Mutex<Option<Memory>>,
}

impl Gauge {
    /// The memory a start is judged against now: the stand-in where one is set, else the
    /// machine's ([`read`]).
    pub fn read(&self) -> Memory {
        let stand_in = *self.stand_in.lock().unwrap_or_else(PoisonError::into_inner);
        stand_in.unwrap_or_else(read)
    }

    /// Whether a reading stands in for the machine's: a test's, which then asks for itself
    /// what a timer would otherwise ask.
    pub fn stood_in(&self) -> bool {
        self.stand_in
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    /// Puts `memory` in place of the machine's reading, or with `None` takes it away.
    pub fn stand_in(&self, memory: Option<Memory>) {
        *self.stand_in.lock().unwrap_or_else(PoisonError::into_inner) = memory;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_is_short_only_at_the_critical_level() {
        assert_eq!(macos_level("1\n"), Memory::Enough);
        assert_eq!(macos_level("2\n"), Memory::Enough, "a warning is not short");
        assert_eq!(macos_level("4\n"), Memory::Short);
        assert_eq!(macos_level(" 8 "), Memory::Short);
        assert_eq!(macos_level("0"), Memory::Unread);
        assert_eq!(macos_level(""), Memory::Unread);
        assert_eq!(macos_level("unknown oid"), Memory::Unread);
    }

    #[test]
    fn linux_is_short_where_every_task_stalls_on_memory_a_tenth_of_the_time() {
        let at = |full: &str| {
            format!(
                "some avg10=55.00 avg60=1.00 avg300=0.50 total=100\n\
                 full avg10={full} avg60=0.00 avg300=0.00 total=12\n"
            )
        };
        assert_eq!(psi(&at("0.00")), Memory::Enough, "some alone is not short");
        assert_eq!(psi(&at("9.99")), Memory::Enough);
        assert_eq!(psi(&at("10.00")), Memory::Short);
        assert_eq!(psi(&at("73.12")), Memory::Short);
        assert_eq!(
            psi("some avg10=1.00 avg60=0.00 avg300=0.00 total=0\n"),
            Memory::Unread,
            "no full line"
        );
        assert_eq!(psi(&at("nan-ish")), Memory::Unread);
    }

    #[test]
    fn linux_without_pressure_information_is_short_under_the_smaller_floor() {
        let info = |total_kib: u64, available_kib: u64| {
            format!(
                "MemTotal:       {total_kib} kB\nMemFree:          1000 kB\n\
                 MemAvailable:   {available_kib} kB\n"
            )
        };
        // 64 GiB: the floor is 512 MiB, not 5% (3.2 GiB).
        let big = 64 * 1024 * 1024;
        assert_eq!(meminfo(&info(big, 3 * 1024 * 1024)), Memory::Enough);
        assert_eq!(meminfo(&info(big, 511 * 1024)), Memory::Short);
        // 4 GiB: 5% (about 205 MiB) is the smaller.
        let small = 4 * 1024 * 1024;
        assert_eq!(meminfo(&info(small, 300 * 1024)), Memory::Enough);
        assert_eq!(meminfo(&info(small, 150 * 1024)), Memory::Short);
        assert_eq!(meminfo("MemTotal: 100 kB\n"), Memory::Unread);
    }

    #[test]
    fn a_stand_in_answers_in_place_of_the_machine_until_it_is_taken_away() {
        let gauge = Gauge::default();
        assert!(!gauge.stood_in());
        gauge.stand_in(Some(Memory::Short));
        assert!(gauge.stood_in());
        assert_eq!(gauge.read(), Memory::Short);
        gauge.stand_in(Some(Memory::Enough));
        assert_eq!(gauge.read(), Memory::Enough);
        gauge.stand_in(None);
        assert!(!gauge.stood_in());
    }

    #[test]
    fn only_short_holds_a_start() {
        assert!(Memory::Short.is_short());
        assert!(!Memory::Enough.is_short());
        assert!(!Memory::Unread.is_short());
    }

    /// Linux's reader reads this machine: a reader that always fell through to unread would
    /// hold nothing, silently. Not asked on macOS, where a sandboxed test run is refused the
    /// `sysctl` read and the answer would say nothing of the reader.
    #[cfg(target_os = "linux")]
    #[test]
    fn this_machine_is_read() {
        assert_ne!(read(), Memory::Unread);
    }
}
