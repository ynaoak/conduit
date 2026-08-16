use async_trait::async_trait;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::plugin_system::traits::ConduitPlugin;
use crate::plugin_system::types::*;

const PLUGIN_ID: &str = "conduit.process-monitor";
const MAX_RESULTS: usize = 15;
/// Ports listed in the subtitle before collapsing into "+N"
const SUBTITLE_PORTS: usize = 3;

/// Processes that Windows cannot survive losing. Terminating any of these
/// bugchecks the machine or makes the session unrecoverable, so kill refuses
/// them outright. explorer.exe and svchost.exe are deliberately absent:
/// explorer restarts itself, and killing a specific svchost is sometimes
/// exactly what the user wants.
const PROTECTED: &[&str] = &[
    "system",
    "system idle process",
    "registry",
    "memory compression",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
];

#[derive(Clone)]
struct ProcessInfo {
    pid: u32,
    name: String,
    /// TCP ports this process is listening on, ascending and deduplicated
    ports: Vec<u16>,
}

pub struct ProcessMonitorPlugin {
    manifest: PluginManifest,
    app: AppHandle,
    matcher: SkimMatcherV2,
}

impl ProcessMonitorPlugin {
    pub fn new(app: AppHandle) -> Self {
        Self {
            manifest: PluginManifest {
                id: PLUGIN_ID.into(),
                name: "Process Monitor".into(),
                description: "Find the process listening on a port and kill it".into(),
                icon: "process".into(),
                keyword: Some("ps".into()),
                // Snapshotting every process is heavier than the other
                // plugins, and kill is destructive — require the keyword
                keyword_only: true,
            },
            app,
            matcher: SkimMatcherV2::default(),
        }
    }

    /// `matched_port` is the port the query hit, so it leads the display
    fn to_result(
        proc: &ProcessInfo,
        matched_port: Option<u16>,
        score: f64,
        match_indices: Vec<usize>,
    ) -> SearchResult {
        let lead_port = matched_port.or_else(|| proc.ports.first().copied());
        let title = match lead_port {
            Some(port) => format!("{} :{}", proc.name, port),
            None => proc.name.clone(),
        };

        let mut subtitle = format!("PID {}", proc.pid);
        if !proc.ports.is_empty() {
            let shown: Vec<String> = proc
                .ports
                .iter()
                .take(SUBTITLE_PORTS)
                .map(|p| format!(":{}", p))
                .collect();
            subtitle.push_str(&format!(" ・ TCP {}", shown.join(", ")));
            if proc.ports.len() > SUBTITLE_PORTS {
                subtitle.push_str(&format!(" +{}", proc.ports.len() - SUBTITLE_PORTS));
            }
        }
        if is_protected(&proc.name) {
            subtitle.push_str(&format!(
                " ・ 🔒 {}",
                crate::i18n::t("plugins", "process_system_protected")
            ));
        }

        SearchResult {
            id: format!("{}:{}", PLUGIN_ID, proc.pid),
            plugin_id: PLUGIN_ID.into(),
            title,
            subtitle: Some(subtitle),
            icon: ResultIcon::Named(if proc.ports.is_empty() {
                "memory".into()
            } else {
                "lan".into()
            }),
            score,
            actions: vec![
                Action {
                    id: "kill".into(),
                    title: crate::i18n::t("plugins", "kill").into(),
                    shortcut: Some("Enter".into()),
                },
                Action {
                    id: "copy-pid".into(),
                    title: crate::i18n::t("plugins", "copy_pid").into(),
                    shortcut: None,
                },
            ],
            match_indices,
        }
    }

    /// Bare `ps`: everything holding a TCP port. Sorting by raw port number
    /// would lead with 135/139/445 — the system services nobody opens this
    /// for — so the registered range (where dev servers live) comes first,
    /// then well-known ports, then the ephemeral range used by RPC.
    fn listening_overview(processes: &[ProcessInfo]) -> Vec<SearchResult> {
        fn rank(port: u16) -> (u8, u16) {
            match port {
                0..=1023 => (1, port),
                1024..=49151 => (0, port),
                _ => (2, port),
            }
        }

        let mut listening: Vec<&ProcessInfo> =
            processes.iter().filter(|p| !p.ports.is_empty()).collect();
        listening.sort_by_key(|p| {
            p.ports
                .iter()
                .map(|port| rank(*port))
                .min()
                .unwrap_or((3, 0))
        });

        listening
            .iter()
            .take(MAX_RESULTS)
            .enumerate()
            .map(|(i, p)| {
                // Lead with the port that earned this row its position
                let lead = p.ports.iter().min_by_key(|port| rank(**port)).copied();
                Self::to_result(p, lead, 0.7 - i as f64 * 0.01, vec![])
            })
            .collect()
    }

    /// Numeric query: processes whose listening ports start with the digits.
    /// An exact port match always outranks a prefix match.
    fn search_by_port(processes: &[ProcessInfo], digits: &str) -> Vec<SearchResult> {
        let exact: Option<u16> = digits.parse().ok();

        let mut results: Vec<(u16, SearchResult)> = processes
            .iter()
            .filter_map(|proc| {
                let hit = proc
                    .ports
                    .iter()
                    .find(|p| Some(**p) == exact)
                    .or_else(|| {
                        proc.ports
                            .iter()
                            .find(|p| p.to_string().starts_with(digits))
                    })
                    .copied()?;

                let score = if Some(hit) == exact { 0.98 } else { 0.75 };
                Some((hit, Self::to_result(proc, Some(hit), score, vec![])))
            })
            .collect();

        results.sort_by(|a, b| {
            b.1.score
                .partial_cmp(&a.1.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        results.truncate(MAX_RESULTS);
        results.into_iter().map(|(_, r)| r).collect()
    }

    /// Text query: fuzzy match on the executable name. Port-bound processes
    /// are boosted — a name search here is usually still about a stuck server.
    fn search_by_name(&self, processes: &[ProcessInfo], query: &str) -> Vec<SearchResult> {
        let mut results: Vec<SearchResult> = processes
            .iter()
            .filter_map(|proc| {
                self.matcher
                    .fuzzy_indices(&proc.name, query)
                    .map(|(score, indices)| {
                        let mut normalized = (score as f64 / 100.0).min(0.9).max(0.0);
                        if !proc.ports.is_empty() {
                            normalized = (normalized + 0.05).min(0.95);
                        }
                        Self::to_result(proc, None, normalized, indices)
                    })
            })
            .collect();

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(MAX_RESULTS);
        results
    }
}

fn is_protected(name: &str) -> bool {
    let name = name.to_lowercase();
    PROTECTED.iter().any(|p| *p == name)
}

#[cfg(windows)]
mod sys {
    use super::ProcessInfo;
    use std::collections::HashMap;

    use windows::Win32::Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
    use windows::Win32::NetworkManagement::IpHelper::{
        GetExtendedTcpTable, MIB_TCP6ROW_OWNER_PID, MIB_TCP6TABLE_OWNER_PID, MIB_TCPROW_OWNER_PID,
        MIB_TCPTABLE_OWNER_PID, TCP_TABLE_OWNER_PID_LISTENER,
    };
    use windows::Win32::Networking::WinSock::{AF_INET, AF_INET6};
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

    /// The table structs are variable-length: a header followed by
    /// `dwNumEntries` rows. Call once to size the buffer, once to fill it.
    unsafe fn tcp_table(family: u32) -> Vec<u8> {
        let mut size: u32 = 0;
        let rc = unsafe {
            GetExtendedTcpTable(
                None,
                &mut size,
                false,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if rc != ERROR_INSUFFICIENT_BUFFER.0 || size == 0 {
            return Vec::new();
        }

        let mut buf = vec![0u8; size as usize];
        let rc = unsafe {
            GetExtendedTcpTable(
                Some(buf.as_mut_ptr() as *mut _),
                &mut size,
                false,
                family,
                TCP_TABLE_OWNER_PID_LISTENER,
                0,
            )
        };
        if rc != NO_ERROR.0 {
            return Vec::new();
        }
        buf
    }

    /// dwLocalPort holds the port in network byte order in its low 16 bits
    fn port_of(raw: u32) -> u16 {
        (((raw & 0xff) << 8) | ((raw >> 8) & 0xff)) as u16
    }

    /// pid -> listening TCP ports, across IPv4 and IPv6
    fn listening_ports() -> HashMap<u32, Vec<u16>> {
        let mut ports: HashMap<u32, Vec<u16>> = HashMap::new();

        unsafe {
            let v4 = tcp_table(AF_INET.0 as u32);
            if v4.len() >= std::mem::size_of::<MIB_TCPTABLE_OWNER_PID>() {
                let table = &*(v4.as_ptr() as *const MIB_TCPTABLE_OWNER_PID);
                let rows = std::slice::from_raw_parts(
                    table.table.as_ptr() as *const MIB_TCPROW_OWNER_PID,
                    table.dwNumEntries as usize,
                );
                for row in rows {
                    ports
                        .entry(row.dwOwningPid)
                        .or_default()
                        .push(port_of(row.dwLocalPort));
                }
            }

            let v6 = tcp_table(AF_INET6.0 as u32);
            if v6.len() >= std::mem::size_of::<MIB_TCP6TABLE_OWNER_PID>() {
                let table = &*(v6.as_ptr() as *const MIB_TCP6TABLE_OWNER_PID);
                let rows = std::slice::from_raw_parts(
                    table.table.as_ptr() as *const MIB_TCP6ROW_OWNER_PID,
                    table.dwNumEntries as usize,
                );
                for row in rows {
                    ports
                        .entry(row.dwOwningPid)
                        .or_default()
                        .push(port_of(row.dwLocalPort));
                }
            }
        }

        // A dual-stack server binds the same port twice (v4 + v6)
        for list in ports.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        ports
    }

    /// One snapshot covers every process, so no per-PID OpenProcess is
    /// needed — that keeps this fast and avoids access-denied gaps.
    pub fn list_processes() -> Vec<ProcessInfo> {
        let mut ports = listening_ports();
        let mut processes = Vec::new();

        unsafe {
            let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else {
                return processes;
            };

            let mut entry = PROCESSENTRY32W {
                dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };

            if Process32FirstW(snapshot, &mut entry).is_ok() {
                loop {
                    let len = entry
                        .szExeFile
                        .iter()
                        .position(|c| *c == 0)
                        .unwrap_or(entry.szExeFile.len());
                    let name = String::from_utf16_lossy(&entry.szExeFile[..len]);

                    processes.push(ProcessInfo {
                        pid: entry.th32ProcessID,
                        name,
                        ports: ports.remove(&entry.th32ProcessID).unwrap_or_default(),
                    });

                    if Process32NextW(snapshot, &mut entry).is_err() {
                        break;
                    }
                }
            }

            let _ = CloseHandle(snapshot);
        }

        processes
    }

    #[cfg(test)]
    mod tests {
        #[test]
        fn swaps_network_byte_order() {
            // 3000 = 0x0BB8, stored byte-swapped as 0xB80B
            assert_eq!(super::port_of(0xB80B), 3000);
            assert_eq!(super::port_of(0x5000), 80);
            assert_eq!(super::port_of(0xFFFF), 65535);
        }

        /// Prints listeners for manual comparison with `netstat -ano`
        #[test]
        fn enumerates_processes_and_ports() {
            let procs = super::list_processes();
            assert!(!procs.is_empty(), "process snapshot was empty");
            assert!(
                procs.iter().any(|p| p.pid == std::process::id()),
                "snapshot is missing the test process itself"
            );
            for p in procs.iter().filter(|p| !p.ports.is_empty()) {
                println!("{:>6}  {:<30} {:?}", p.pid, p.name, p.ports);
            }
        }

        #[test]
        fn terminates_a_live_process() {
            let mut child = std::process::Command::new("cmd")
                .args(["/c", "ping -n 60 127.0.0.1 > NUL"])
                .spawn()
                .expect("failed to spawn test child");
            let pid = child.id();

            assert!(
                super::list_processes().iter().any(|p| p.pid == pid),
                "spawned child is missing from the snapshot"
            );

            super::kill(pid).expect("kill failed");

            let status = child.wait().expect("wait failed");
            assert!(!status.success(), "child exited cleanly, it was not killed");
        }
    }

    pub fn kill(pid: u32) -> anyhow::Result<()> {
        unsafe {
            let handle = OpenProcess(PROCESS_TERMINATE, false, pid)
                .map_err(|e| {
                    anyhow::anyhow!(crate::i18n::tf(
                        "errors",
                        "process_open",
                        &[("pid", &pid.to_string()), ("error", &e.to_string())]
                    ))
                })?;
            let result = TerminateProcess(handle, 1)
                .map_err(|e| {
                    anyhow::anyhow!(crate::i18n::tf(
                        "errors",
                        "process_kill",
                        &[("pid", &pid.to_string()), ("error", &e.to_string())]
                    ))
                });
            let _ = CloseHandle(handle);
            result
        }
    }
}

#[cfg(not(windows))]
mod sys {
    use super::ProcessInfo;

    pub fn list_processes() -> Vec<ProcessInfo> {
        Vec::new()
    }

    pub fn kill(_pid: u32) -> anyhow::Result<()> {
        anyhow::bail!(crate::i18n::t("errors", "process_windows_only"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<ProcessInfo> {
        vec![
            ProcessInfo {
                pid: 100,
                name: "node.exe".into(),
                ports: vec![3000, 5173],
            },
            ProcessInfo {
                pid: 200,
                name: "python.exe".into(),
                ports: vec![30001],
            },
            ProcessInfo {
                pid: 300,
                name: "chrome.exe".into(),
                ports: vec![],
            },
            ProcessInfo {
                pid: 4,
                name: "System".into(),
                ports: vec![445],
            },
        ]
    }

    #[test]
    fn exact_port_outranks_prefix() {
        let results = ProcessMonitorPlugin::search_by_port(&sample(), "3000");
        assert_eq!(results.len(), 2, "expected the :3000 and :30001 processes");
        // node.exe listens on exactly 3000; python.exe only prefix-matches
        assert_eq!(results[0].title, "node.exe :3000");
        assert_eq!(results[1].title, "python.exe :30001");
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn port_search_ignores_portless_processes() {
        let results = ProcessMonitorPlugin::search_by_port(&sample(), "44");
        assert!(results.iter().all(|r| r.title != "chrome.exe"));
    }

    #[test]
    fn matched_port_leads_the_title() {
        // 5173 is node's second port, but it is what the query hit
        let results = ProcessMonitorPlugin::search_by_port(&sample(), "5173");
        assert_eq!(results[0].title, "node.exe :5173");
        assert_eq!(
            results[0].subtitle.as_deref(),
            Some("PID 100 ・ TCP :3000, :5173")
        );
    }

    #[test]
    fn overview_ranks_dev_ports_above_system_ports() {
        let results = ProcessMonitorPlugin::listening_overview(&sample());
        let titles: Vec<&str> = results.iter().map(|r| r.title.as_str()).collect();
        // chrome.exe has no ports and is excluded; System :445 is well-known
        // so it sorts below the registered-range listeners
        assert_eq!(
            titles,
            vec!["node.exe :3000", "python.exe :30001", "System :445"]
        );
    }

    #[test]
    fn protected_processes_are_flagged() {
        assert!(is_protected("System"));
        assert!(is_protected("LSASS.EXE"));
        assert!(!is_protected("node.exe"));
        assert!(!is_protected("explorer.exe"));

        let results = ProcessMonitorPlugin::search_by_port(&sample(), "445");
        assert!(results[0]
            .subtitle
            .as_deref()
            .unwrap()
            .contains(crate::i18n::t("plugins", "process_system_protected")));
    }
}

#[async_trait]
impl ConduitPlugin for ProcessMonitorPlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    async fn search(&self, query: &str) -> Vec<SearchResult> {
        let query = query.trim().to_string();
        let processes = tokio::task::spawn_blocking(sys::list_processes)
            .await
            .unwrap_or_default();

        if query.is_empty() {
            return Self::listening_overview(&processes);
        }

        // A bare number is a port lookup; anything else is a name lookup
        if query.chars().all(|c| c.is_ascii_digit()) {
            return Self::search_by_port(&processes, &query);
        }
        self.search_by_name(&processes, &query)
    }

    async fn execute(&self, result_id: &str, action_id: &str) -> anyhow::Result<()> {
        let pid: u32 = result_id
            .strip_prefix(&format!("{}:", PLUGIN_ID))
            .unwrap_or(result_id)
            .parse()
            .map_err(|_| anyhow::anyhow!("invalid process id: {}", result_id))?;

        if action_id == "copy-pid" {
            self.app.clipboard().write_text(pid.to_string())?;
            return Ok(());
        }

        // Guard the kill path only — copying a PID is always harmless
        if pid == std::process::id() {
            anyhow::bail!(crate::i18n::t("errors", "process_self"));
        }
        if pid <= 4 {
            anyhow::bail!(crate::i18n::tf(
                "errors",
                "process_system",
                &[("pid", &pid.to_string())]
            ));
        }

        // Re-read the name from a fresh snapshot: the result may be stale,
        // and a recycled PID must not be killed under the old identity
        let processes = tokio::task::spawn_blocking(sys::list_processes)
            .await
            .unwrap_or_default();
        let Some(proc) = processes.iter().find(|p| p.pid == pid) else {
            anyhow::bail!(crate::i18n::tf(
                "errors",
                "process_gone",
                &[("pid", &pid.to_string())]
            ));
        };
        if is_protected(&proc.name) {
            anyhow::bail!(crate::i18n::tf(
                "errors",
                "process_protected_name",
                &[("name", &proc.name)]
            ));
        }

        tokio::task::spawn_blocking(move || sys::kill(pid))
            .await
            .map_err(|e| anyhow::anyhow!("kill task failed: {}", e))?
    }
}
