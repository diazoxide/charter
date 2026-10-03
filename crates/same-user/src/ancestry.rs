//! Who a process was started under: the one parent-chain walker charter has (V82's commit
//! stamping and HP-6's permission hook both climb it), and the host check a hook makes with it.
//!
//! **Read from the kernel, never from `PATH`.** Linux answers from `/proc`. Everywhere else on
//! unix it is a tool by its absolute path with no environment (`/bin/ps`, `/usr/sbin/lsof`):
//! macOS keeps no `/proc`, and reading another process's parent or its descriptors there
//! without those tools is `libproc`, which is `unsafe` this workspace forbids. Every doubt
//! answers no.

use std::collections::HashMap;
use std::io;
use std::os::fd::AsFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use crate::{NotThisUser, Uid, admit_peer, peer_process_of};

/// The most generations a walk climbs. A real chain is a handful; a table that loops, or a tree
/// deeper than this, is not one to vouch for.
pub const MOST_GENERATIONS: usize = 64;

/// Every process's parent.
#[derive(Debug, Clone)]
pub enum Parents {
    /// Linux: `/proc`, asked one pid at a time.
    Proc,
    /// Everywhere else: every process's parent, as `ps` listed them once.
    Table(HashMap<u32, u32>),
}

/// The command that lists every process and its parent: `/bin/ps` by its full path, with no
/// environment.
pub fn ps_command() -> Command {
    let mut ps = Command::new("/bin/ps");
    ps.args(["-A", "-o", "pid=", "-o", "ppid="])
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    ps
}

impl Parents {
    /// The table now, with `run` running [`ps_command`] where the platform needs it. `run` is the
    /// caller's way to start a process (charter-core starts every one under its fork lock).
    pub fn now(run: impl FnOnce(&mut Command) -> io::Result<Output>) -> io::Result<Parents> {
        if cfg!(any(target_os = "linux", target_os = "android")) {
            return Ok(Parents::Proc);
        }
        let out = run(&mut ps_command())?;
        if !out.status.success() {
            return Err(io::Error::other("ps did not list the processes"));
        }
        Ok(Parents::Table(ps_lines(&String::from_utf8_lossy(
            &out.stdout,
        ))))
    }

    /// The parent of `pid`, where it is known.
    pub fn of(&self, pid: u32) -> Option<u32> {
        match self {
            Parents::Proc => proc_parent(pid),
            Parents::Table(table) => table.get(&pid).copied(),
        }
    }

    /// `start`'s ancestors, its parent first, up to and without pid 1 (`init`, `launchd`), which
    /// is above everything and so vouches for nothing.
    pub fn chain(&self, start: u32) -> Vec<u32> {
        let mut chain = Vec::new();
        let mut at = start;
        for _ in 0..MOST_GENERATIONS {
            match self.of(at) {
                Some(up) if up > 1 && up != at && !chain.contains(&up) => {
                    chain.push(up);
                    at = up;
                }
                _ => break,
            }
        }
        chain
    }
}

/// Climbs from `start` through `parent` until it meets `ancestor`, runs out, or loops.
pub fn walk(start: u32, ancestor: u32, parent: impl Fn(u32) -> Option<u32>) -> bool {
    let mut at = start;
    for _ in 0..MOST_GENERATIONS {
        if at == ancestor {
            return true;
        }
        match parent(at) {
            // Pid 0 is the kernel's, the parent of `init` and of `launchd`: the top.
            Some(up) if up != 0 && up != at => at = up,
            _ => return false,
        }
    }
    false
}

/// The parent `/proc/<pid>/stat` names.
fn proc_parent(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    stat_parent(&stat)
}

/// The parent in one `/proc/<pid>/stat` line: the fourth field, counted after the command's
/// closing parenthesis, because the command itself may hold spaces and parentheses.
pub fn stat_parent(stat: &str) -> Option<u32> {
    let (_, after) = stat.rsplit_once(')')?;
    after.split_whitespace().nth(1)?.parse().ok()
}

/// `pid ppid` lines, as [`ps_command`] prints them; a line that is not two numbers is skipped.
pub fn ps_lines(text: &str) -> HashMap<u32, u32> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            Some((words.next()?.parse().ok()?, words.next()?.parse().ok()?))
        })
        .collect()
}

/// Why the far end of a socket this process connected to is not the host it expects.
#[derive(Debug, thiserror::Error)]
pub enum NotOurHost {
    #[error("{0}")]
    NotThisUser(NotThisUser),
    /// The process listening is not one of this process's ancestors.
    #[error("the process listening there (pid {pid}) is not one this process was started under")]
    NotAnAncestor { pid: u32 },
    /// The ancestor the socket's peer pid names holds no socket bound at the path: the pid is
    /// one a dead listener left behind, now another process's.
    #[error(
        "the process the socket names (pid {pid}) holds no socket at that path, so it is not \
         the one listening there"
    )]
    HoldsNoSocketThere { pid: u32 },
    #[error("this process's ancestors could not be read: {0}")]
    NoAncestry(io::Error),
    #[error("which sockets the listener holds could not be read: {0}")]
    NoSocketTable(io::Error),
}

/// `Ok` when the process listening at the far end of `socket`, connected at `path`, runs as this
/// user, IS one of this process's ancestors, and HOLDS a socket bound at `path` now (FD-6's
/// mutual check, for a socket whose reply carries authority, ADR 0068 amended by HP-6).
///
/// **The peer pid alone is not an identity.** On both kernels it is the number of the process
/// that listened, kept after that process is gone (`SO_PEERCRED`'s pid, macOS's
/// `LOCAL_PEERPID`), and a later process can be given the same number. So the process with that
/// number must also hold the listening socket itself, read from the kernel's own tables:
/// `/proc/net/unix` and `/proc/<pid>/fd` on Linux, `/usr/sbin/lsof` on macOS. A process that was
/// given a dead listener's pid holds no such socket, and nothing a chat runs can hand one to
/// an ancestor. No timestamp is trusted: the socket's are its owner's to set.
///
/// No secret is needed, and none would hold: whatever this process knows, its siblings started
/// from the same environment know too. This process itself never counts: a hook only connects.
pub fn admit_host(
    socket: &impl AsFd,
    path: &Path,
    mut run: impl FnMut(&mut Command) -> io::Result<Output>,
) -> Result<(), NotOurHost> {
    let (uid, pid) = match peer_process_of(socket) {
        Ok(peer) => peer,
        Err(why) => {
            return Err(NotOurHost::NotThisUser(NotThisUser::Unidentified(why)));
        }
    };
    admit_peer(Ok(uid), Uid::effective()).map_err(NotOurHost::NotThisUser)?;
    let parents = Parents::now(&mut run).map_err(NotOurHost::NoAncestry)?;
    let ancestors = parents.chain(std::process::id());
    if !ancestors.contains(&pid) {
        return Err(NotOurHost::NotAnAncestor { pid });
    }
    let holds = holds_a_socket_at(pid, path, &mut run).map_err(NotOurHost::NoSocketTable)?;
    judge(pid, &ancestors, holds)
}

/// The decision [`admit_host`] makes, given what it read: the listener's pid, this process's
/// ancestors, and whether the process with that pid holds a socket bound at the path.
pub fn judge(pid: u32, ancestors: &[u32], holds_the_socket: bool) -> Result<(), NotOurHost> {
    if !ancestors.contains(&pid) {
        return Err(NotOurHost::NotAnAncestor { pid });
    }
    if !holds_the_socket {
        return Err(NotOurHost::HoldsNoSocketThere { pid });
    }
    Ok(())
}

/// Whether process `pid` holds a unix socket bound at `path` now. Paths are compared once both
/// are resolved, so `/tmp` and `/private/tmp`, or a link to the socket, name the same one.
pub fn holds_a_socket_at(
    pid: u32,
    path: &Path,
    run: impl FnOnce(&mut Command) -> io::Result<Output>,
) -> io::Result<bool> {
    let target = std::fs::canonicalize(path)?;
    if cfg!(any(target_os = "linux", target_os = "android")) {
        let table = std::fs::read_to_string("/proc/net/unix")?;
        let inodes = listening_at(&table, &target);
        if inodes.is_empty() {
            return Ok(false);
        }
        for fd in std::fs::read_dir(format!("/proc/{pid}/fd"))? {
            let Ok(link) = std::fs::read_link(fd?.path()) else {
                continue;
            };
            if let Some(inode) = socket_inode(&link)
                && inodes.contains(&inode)
            {
                return Ok(true);
            }
        }
        return Ok(false);
    }
    let out = run(&mut lsof_command(pid))?;
    // `lsof` exits 1 when the process holds no unix socket at all: an answer, not a failure,
    // as long as it printed nothing. Any other failure is one.
    if !out.status.success() && !(out.status.code() == Some(1) && out.stdout.is_empty()) {
        return Err(io::Error::other("lsof did not list the process's sockets"));
    }
    Ok(lsof_names(&String::from_utf8_lossy(&out.stdout))
        .iter()
        .any(|name| resolves_to(name, &target)))
}

/// `lsof` asked for the unix sockets of process `pid` alone, by its full path, with no
/// environment.
pub fn lsof_command(pid: u32) -> Command {
    let mut lsof = Command::new("/usr/sbin/lsof");
    lsof.args(["-nP", "-a", "-p", &pid.to_string(), "-U", "-F", "n"])
        .env_clear()
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    lsof
}

/// The names `lsof -F n` printed (`n<name>` lines) that are paths: a socket bound at a path,
/// listening or accepted from it. A connected client's is `->0x…`, never a path.
pub fn lsof_names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix('n'))
        .filter(|name| name.starts_with('/'))
        .map(str::to_owned)
        .collect()
}

/// The inodes of `/proc/net/unix`'s listening sockets bound at a path that resolves to
/// `target`. A listening socket carries `__SO_ACCEPTCON` (`0x10000`) in its flags.
pub fn listening_at(table: &str, target: &Path) -> Vec<u64> {
    table
        .lines()
        .skip(1)
        .filter_map(|line| {
            // Num RefCount Protocol Flags Type St Inode Path
            let fields: Vec<&str> = line.split_whitespace().collect();
            let flags = u64::from_str_radix(fields.get(3)?, 16).ok()?;
            let inode = fields.get(6)?.parse().ok()?;
            let path = fields.get(7..)?.join(" ");
            (flags & 0x10000 != 0 && resolves_to(&path, target)).then_some(inode)
        })
        .collect()
}

/// The inode of a `/proc/<pid>/fd` link that is a socket: `socket:[<inode>]`.
fn socket_inode(link: &Path) -> Option<u64> {
    link.to_str()?
        .strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

/// Whether `name`, a path a socket was bound at, resolves to `target`. A path that no longer
/// resolves names nothing.
fn resolves_to(name: &str, target: &Path) -> bool {
    !name.is_empty() && std::fs::canonicalize(PathBuf::from(name)).is_ok_and(|at| at == target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ancestor_that_holds_the_socket_is_the_host() {
        assert!(judge(10, &[30, 20, 10], true).is_ok());
    }

    #[test]
    fn an_ancestor_that_reused_a_dead_listener_s_pid_holds_no_socket_and_is_refused() {
        // A stand-in listened, left its socket with a child and exited; a chat's harness
        // started later and was given the same pid. It is an ancestor, and holds nothing.
        assert!(matches!(
            judge(30, &[30, 20, 10], false),
            Err(NotOurHost::HoldsNoSocketThere { pid: 30 })
        ));
    }

    #[test]
    fn a_process_that_is_no_ancestor_is_never_the_host() {
        assert!(matches!(
            judge(99, &[30, 20, 10], true),
            Err(NotOurHost::NotAnAncestor { pid: 99 })
        ));
    }

    #[test]
    fn a_process_holding_a_socket_at_the_path_is_told_apart_from_one_that_holds_none() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("s.sock");
        let _listener = std::os::unix::net::UnixListener::bind(&path).expect("bound");
        let run = |command: &mut Command| command.output();

        assert!(holds_a_socket_at(std::process::id(), &path, run).expect("readable"));
        // The test's parent (cargo, or whatever ran it) holds nothing there.
        assert!(
            !holds_a_socket_at(std::os::unix::process::parent_id(), &path, run).expect("readable")
        );
    }

    #[test]
    fn proc_net_unix_names_only_its_listening_sockets_at_the_path() {
        let dir = tempfile::tempdir().expect("a directory");
        let path = dir.path().join("hooks.sock");
        std::fs::write(&path, "").expect("a file to resolve");
        let target = std::fs::canonicalize(&path).expect("resolves");
        let shown = path.display();
        let table = format!(
            "Num       RefCount Protocol Flags    Type St Inode Path\n\
             0000000000000000: 00000002 00000000 00010000 0001 01 4242 {shown}\n\
             0000000000000000: 00000003 00000000 00000000 0001 03 4343 {shown}\n\
             0000000000000000: 00000002 00000000 00010000 0001 01 4444 /elsewhere.sock\n\
             0000000000000000: 00000002 00000000 00010000 0001 01 4545\n"
        );

        assert_eq!(listening_at(&table, &target), [4242]);
        assert_eq!(socket_inode(Path::new("socket:[4242]")), Some(4242));
        assert_eq!(socket_inode(Path::new("pipe:[4242]")), None);
    }

    #[test]
    fn lsof_names_are_the_paths_and_never_a_client_s_arrow() {
        assert_eq!(
            lsof_names("p85072\nf3\nn/tmp/a.sock\nf4\nn->0x4f16079b877a2aa\nf5\nn/tmp/a.sock\n"),
            ["/tmp/a.sock", "/tmp/a.sock"]
        );
    }

    #[test]
    fn ps_lines_read_each_parent() {
        assert_eq!(
            ps_lines("  1     0\n 4242   17\nnot a line\n"),
            [(1, 0), (4242, 17)].into_iter().collect()
        );
    }

    #[test]
    fn a_walk_stops_at_the_top_at_a_gap_and_at_a_loop() {
        let tree = |pid| match pid {
            30 => Some(20),
            20 => Some(10),
            10 => Some(1),
            1 => Some(0),
            7 => Some(7),
            8 => Some(9),
            9 => Some(8),
            _ => None,
        };
        assert!(walk(30, 10, tree));
        assert!(walk(30, 1, tree));
        assert!(!walk(30, 99, tree), "the chain ends at the top");
        assert!(!walk(20, 30, tree), "a child is not an ancestor");
        assert!(!walk(55, 10, tree), "no parent known");
        assert!(!walk(7, 99, tree), "its own parent");
        assert!(!walk(8, 10, tree), "a loop");
    }

    #[test]
    fn stat_parent_reads_after_the_last_parenthesis() {
        assert_eq!(stat_parent("123 (a (b) c) S 45 6 7"), Some(45));
        assert_eq!(stat_parent("garbage"), None);
    }
}
