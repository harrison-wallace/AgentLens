//! Serializable types crossing the UI <-> backend boundary.
//!
//! Everything the frontend sends or receives is defined here and mirrored in
//! `src/lib/protocol.ts`. This is deliberate: a later phase replaces the
//! in-process backend with a remote daemon, and only serializable messages
//! survive that move.

use serde::{Deserialize, Serialize};

/// Identity of the running application, surfaced in the window title and UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: String,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_serializes_with_camel_case_fields() {
        let info = AppInfo {
            name: "AgentLens".into(),
            version: "0.0.1".into(),
        };

        let json = serde_json::to_value(&info).unwrap();
        assert_eq!(json["name"], "AgentLens");
        assert_eq!(json["version"], "0.0.1");
    }
}
