//! qxp: structured connectivity report mirror types.
//!
//! Plain JSON-friendly counterparts of the types defined in
//! [`deltachat::ConnectivityReport`] et al. `From` conversions keep the api
//! method body trivial.

use deltachat::context::{
    ConnectivityDot as CoreDot, ConnectivityLine as CoreLine,
    ConnectivityQuotaInfo as CoreQuota, ConnectivityReport as CoreReport,
    ConnectivityTransportReport as CoreTransport,
};
use serde::Serialize;
use typescript_type_def::TypeDef;

#[derive(Serialize, TypeDef, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ConnectivityDot {
    Green,
    Yellow,
    Red,
    Gray,
}

#[derive(Serialize, TypeDef, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityLine {
    pub dot: ConnectivityDot,
    pub text: String,
}

#[derive(Serialize, TypeDef, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityQuotaInfo {
    pub percent: u32,
    pub label: String,
}

#[derive(Serialize, TypeDef, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityTransportReport {
    pub addr: String,
    pub lines: Vec<ConnectivityLine>,
    pub quota: Option<ConnectivityQuotaInfo>,
}

#[derive(Serialize, TypeDef, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ConnectivityReport {
    pub transports: Vec<ConnectivityTransportReport>,
    pub smtp: ConnectivityLine,
}

impl From<CoreDot> for ConnectivityDot {
    fn from(d: CoreDot) -> Self {
        match d {
            CoreDot::Green => Self::Green,
            CoreDot::Yellow => Self::Yellow,
            CoreDot::Red => Self::Red,
            CoreDot::Gray => Self::Gray,
        }
    }
}

impl From<CoreLine> for ConnectivityLine {
    fn from(l: CoreLine) -> Self {
        Self {
            dot: l.dot.into(),
            text: l.text,
        }
    }
}

impl From<CoreQuota> for ConnectivityQuotaInfo {
    fn from(q: CoreQuota) -> Self {
        Self {
            percent: q.percent,
            label: q.label,
        }
    }
}

impl From<CoreTransport> for ConnectivityTransportReport {
    fn from(t: CoreTransport) -> Self {
        Self {
            addr: t.addr,
            lines: t.lines.into_iter().map(Into::into).collect(),
            quota: t.quota.map(Into::into),
        }
    }
}

impl From<CoreReport> for ConnectivityReport {
    fn from(r: CoreReport) -> Self {
        Self {
            transports: r.transports.into_iter().map(Into::into).collect(),
            smtp: r.smtp.into(),
        }
    }
}
