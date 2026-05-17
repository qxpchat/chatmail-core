//! qxp: structured connectivity report.
//!
//! Mirrors the data that [`Context::get_connectivity_html`] renders into HTML,
//! but returns typed Rust values instead so clients can render their own UI
//! without parsing the HTML blob. The HTML path is left untouched.
//!
//! Re-exported from [`crate::context`] as `Connectivity*` so this stays an
//! additive sibling module — upstream rebases only touch the four
//! `pub(super)` visibility tweaks in `connectivity.rs` and a single
//! `mod connectivity_report;` line in `scheduler.rs`.
//!
//! [`Context::get_connectivity_html`]: crate::context::Context::get_connectivity_html
//! [`crate::context`]: crate::context

use anyhow::Result;
use humansize::{BINARY, format_size};

use super::InnerSchedulerState;
use super::connectivity::{ConnectivityStore, DetailedConnectivity};
use crate::context::Context;
use crate::stock_str;

/// Status-dot color for a [`ConnectivityLine`]. Matches the four `.dot` classes
/// the HTML report uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectivityDot {
    /// Operational / connected.
    Green,
    /// Connecting or other transient state.
    Yellow,
    /// Error or not-configured state.
    Red,
    /// Inactive / informational (e.g. not started, low quota usage).
    Gray,
}

/// One status row of a [`ConnectivityReport`] — a dot color paired with a
/// pre-localized human-readable label (same stock strings as the HTML report,
/// in the account's configured language).
#[derive(Debug, Clone)]
pub struct ConnectivityLine {
    /// Status-dot color for this row.
    pub dot: ConnectivityDot,
    /// Pre-localized human-readable status text.
    pub text: String,
}

/// Quota info for one transport. Reflects the highest-usage resource across all
/// roots reported by the IMAP server.
#[derive(Debug, Clone)]
pub struct ConnectivityQuotaInfo {
    /// Server-reported usage percentage (`0..=100` typically; can overshoot if
    /// the server reports more usage than its declared limit).
    pub percent: u32,
    /// Pre-localized label (e.g. "1.34 GiB of 2 GiB used").
    pub label: String,
}

/// Status report for a single transport in [`ConnectivityReport`].
#[derive(Debug, Clone)]
pub struct ConnectivityTransportReport {
    /// Email address of the transport (exact join key with the `transports` table).
    pub addr: String,
    /// Per-folder + inbox-error status lines, in the order the HTML report
    /// renders them.
    pub lines: Vec<ConnectivityLine>,
    /// Highest-usage quota resource for this transport, or `None` when the
    /// server doesn't support quota or returns no useful data.
    pub quota: Option<ConnectivityQuotaInfo>,
}

/// Structured connectivity diagnostics.
///
/// Equivalent of [`Context::get_connectivity_html`] minus presentation. Emits
/// the same [`crate::events::EventType::ConnectivityChanged`] event when the
/// underlying data changes.
#[derive(Debug, Clone)]
pub struct ConnectivityReport {
    /// One entry per transport in the `transports` table.
    pub transports: Vec<ConnectivityTransportReport>,
    /// Rolled-up SMTP / outgoing-message status.
    pub smtp: ConnectivityLine,
}

fn dot_for(d: &DetailedConnectivity) -> ConnectivityDot {
    match d {
        DetailedConnectivity::Error(_) | DetailedConnectivity::Uninitialized => {
            ConnectivityDot::Red
        }
        DetailedConnectivity::Connecting => ConnectivityDot::Yellow,
        DetailedConnectivity::Preparing
        | DetailedConnectivity::Working
        | DetailedConnectivity::InterruptingIdle
        | DetailedConnectivity::Idle => ConnectivityDot::Green,
    }
}

impl Context {
    /// Structured equivalent of [`Self::get_connectivity_html`].
    ///
    /// Source data and string formatting match the HTML report exactly, so
    /// clients can swap from `get_connectivity_html` to this without losing
    /// information. When IO isn't running, each transport gets a single
    /// "Not connected" line so the UI never falls back to nothing.
    pub async fn get_connectivity_report(&self) -> Result<ConnectivityReport> {
        // Snapshot scheduler state — same source as get_connectivity_html.
        // Upstream 2.50.0 dropped `SchedBox::meaning` (the typed
        // `FolderMeaning` enum) along with the `FolderMeaning::to_config`
        // helper and the `get_watched_folder_configs` query; each scheduler
        // box now just carries the folder name as a `String`. We mirror
        // that here: one line per box with the transport domain as the
        // label (matching what `get_connectivity_html` renders).
        let lock = self.scheduler.inner.read().await;
        let (folders_states, smtp_state): (
            Option<Vec<(String, ConnectivityStore)>>,
            Option<ConnectivityStore>,
        ) = match *lock {
            InnerSchedulerState::Started(ref sched) => (
                Some(
                    sched
                        .boxes()
                        .map(|b| (b.addr.clone(), b.conn_state.state.connectivity.clone()))
                        .collect(),
                ),
                Some(sched.smtp.state.connectivity.clone()),
            ),
            _ => (None, None),
        };
        drop(lock);

        let transports = self
            .sql
            .query_map_vec("SELECT id, addr FROM transports", (), |row| {
                let transport_id: u32 = row.get(0)?;
                let addr: String = row.get(1)?;
                Ok((transport_id, addr))
            })
            .await?;

        let quota_guard = self.quota.read().await;

        let mut transports_out = Vec::with_capacity(transports.len());
        for (transport_id, transport_addr) in transports {
            let domain = deltachat_contact_tools::EmailAddress::new(&transport_addr)
                .map_or(transport_addr.clone(), |email| email.domain);

            let mut lines: Vec<ConnectivityLine> = Vec::new();

            match &folders_states {
                None => {
                    lines.push(ConnectivityLine {
                        dot: ConnectivityDot::Gray,
                        text: stock_str::not_connected(self),
                    });
                }
                Some(states) => {
                    for (_addr, state) in states
                        .iter()
                        .filter(|(folder_addr, ..)| *folder_addr == transport_addr)
                    {
                        let detailed = state.get_detailed();
                        lines.push(ConnectivityLine {
                            dot: dot_for(&detailed),
                            text: format!("{domain}: {}", detailed.to_string_imap(self)),
                        });
                    }
                }
            }

            // Pick the highest-usage resource across all quota roots.
            let quota = match quota_guard
                .get(&transport_id)
                .and_then(|q| q.recent.as_ref().ok())
            {
                Some(roots) if !roots.is_empty() => {
                    let mut best: Option<(u64, &async_imap::types::QuotaResource)> = None;
                    for resources in roots.values() {
                        for r in resources {
                            let p = r.get_usage_percentage();
                            if best.is_none_or(|(bp, _)| p >= bp) {
                                best = Some((p, r));
                            }
                        }
                    }
                    best.map(|(percent, r)| {
                        use async_imap::types::QuotaResourceName::*;
                        let label = match &r.name {
                            Storage => {
                                let usage = format_size(r.usage * 1024, BINARY);
                                let limit = format_size(r.limit * 1024, BINARY);
                                stock_str::part_of_total_used(self, &usage, &limit)
                            }
                            _ => stock_str::part_of_total_used(
                                self,
                                &r.usage.to_string(),
                                &r.limit.to_string(),
                            ),
                        };
                        ConnectivityQuotaInfo {
                            percent: percent as u32,
                            label,
                        }
                    })
                }
                _ => None,
            };

            transports_out.push(ConnectivityTransportReport {
                addr: transport_addr,
                lines,
                quota,
            });
        }
        drop(quota_guard);

        let smtp = match smtp_state {
            Some(state) => {
                let detailed = state.get_detailed();
                ConnectivityLine {
                    dot: dot_for(&detailed),
                    text: detailed.to_string_smtp(self),
                }
            }
            None => ConnectivityLine {
                dot: ConnectivityDot::Gray,
                text: stock_str::not_connected(self),
            },
        };

        Ok(ConnectivityReport {
            transports: transports_out,
            smtp,
        })
    }
}
