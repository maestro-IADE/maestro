//! Memory watch (docs/features/agents/away-stop.md "메모리 경고"): every few seconds, how much memory the computer
//! has left and how much the agents' processes take. When it runs low while agents are on, the main window's top
//! bar warns ("maestro://memory") and offers to turn the idle ones off (`turn_off_idle` in main.rs).

use crate::agent::Proc;
use serde::Serialize;
use std::collections::HashMap;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

const GIB: u64 = 1 << 30;

/// What the warning shows. Sizes in MB.
#[derive(Serialize, Clone, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// below the line (`low_now`): the warning shows while agents are on
    pub low: bool,
    pub total_mb: u64,
    pub available_mb: u64,
    /// every agent process tree together
    pub agents_mb: u64,
    /// agents whose process runs
    pub agents_on: usize,
    /// of those, how many 쉬는 에이전트 끄기 would turn off now
    pub idle: usize,
}

/// The free memory under which it counts as low: 15% of the computer's, kept between 1.5 and 4 GB (a big machine
/// isn't short with 5 GB free; a small one is already short before 15%).
pub fn line(total: u64) -> u64 {
    (total * 15 / 100).clamp(GIB * 3 / 2, 4 * GIB)
}

/// Low now, given whether it was low before: it goes low under the line and fine again only a quarter above it,
/// so a machine sitting on the line doesn't make the warning come and go.
pub fn low_now(was_low: bool, total: u64, available: u64) -> bool {
    let l = line(total);
    if was_low { available < l + l / 4 } else { available < l }
}

/// One look at the computer: its memory, and every process (as `agent::Proc`, with its memory in bytes beside it).
pub struct Look {
    pub total: u64,
    pub available: u64,
    pub procs: Vec<Proc>,
    pub mem: HashMap<u32, u64>,
}

/// Reads memory and the process list. Kept between looks: sysinfo refreshes faster on a `System` it already filled.
pub struct Watch {
    sys: Option<System>,
}

impl Watch {
    pub fn new() -> Self {
        Self { sys: System::new().ok() }
    }

    pub fn look(&mut self) -> Option<Look> {
        let sys = self.sys.as_mut()?;
        sys.refresh_memory();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_memory());
        let mut procs = Vec::new();
        let mut mem = HashMap::new();
        for (pid, p) in sys.processes() {
            let pid = pid.as_u32();
            procs.push(Proc {
                pid,
                ppid: p.parent().map_or(0, |x| x.as_u32()),
                name: p.name().to_string_lossy().into_owned(),
                started: p.start_time() * 1000,
            });
            mem.insert(pid, p.memory());
        }
        Some(Look { total: sys.total_memory(), available: sys.available_memory(), procs, mem })
    }
}

/// Bytes taken by `root` and everything below it started since `since` (ms, `Runtime::started`): the parent id of
/// an older process belonged to another one before the pid was reused (`agent::born_since`).
pub fn tree_bytes(procs: &[Proc], mem: &HashMap<u32, u64>, root: u32, since: u64) -> u64 {
    let mut kids: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in procs.iter().filter(|p| crate::agent::born_since(p, since)) {
        if p.pid != p.ppid {
            kids.entry(p.ppid).or_default().push(p.pid);
        }
    }
    let mut total = 0;
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![root];
    while let Some(pid) = stack.pop() {
        if !seen.insert(pid) {
            continue;
        }
        total += mem.get(&pid).copied().unwrap_or(0);
        if let Some(k) = kids.get(&pid) {
            stack.extend(k);
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = GIB;

    #[test]
    fn the_line_is_a_share_kept_between_bounds() {
        assert_eq!(line(8 * GB), GB * 3 / 2, "15% of 8 GB is 1.2 GB: raised to 1.5");
        assert_eq!(line(16 * GB), 16 * GB * 15 / 100);
        assert_eq!(line(64 * GB), 4 * GB, "15% of 64 GB is 9.6 GB: capped at 4");
    }

    #[test]
    fn low_comes_and_goes_with_a_margin() {
        let t = 16 * GB;
        let l = line(t);
        assert!(!low_now(false, t, l));
        assert!(low_now(false, t, l - 1));
        assert!(low_now(true, t, l + 1), "just above the line it stays low");
        assert!(!low_now(true, t, l + l / 4), "a quarter above, fine again");
    }

    #[test]
    fn a_tree_counts_its_children_not_older_strangers() {
        let p = |pid, ppid, started| Proc { pid, ppid, name: "x".into(), started };
        // 10 is the agent (started at 5000), 11 and 12 its children, 13 a grandchild; 20 is an old process whose
        // parent id happens to be 10 (from before the pid was reused)
        let procs = vec![p(10, 1, 5000), p(11, 10, 6000), p(12, 10, 7000), p(13, 11, 8000), p(20, 10, 1000), p(30, 1, 9000)];
        let mem: HashMap<u32, u64> = [(10, 100), (11, 20), (12, 30), (13, 4), (20, 999), (30, 999)].into_iter().collect();
        assert_eq!(tree_bytes(&procs, &mem, 10, 5000), 154);
        assert_eq!(tree_bytes(&procs, &mem, 99, 0), 0, "a pid not listed adds nothing");
    }
}
