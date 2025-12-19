//! Entry dumping functionality for debugging.
//

use super::Entry;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

impl Entry {
    /// Converts the entry to a map for JSON serialization.
    pub fn to_map(&self) -> serde_json::Value {
        let payload_result = self.payload();
        let payload_map = match payload_result {
            Ok(p) => {
                let mut queries_map = HashMap::new();
                for (k, v) in &p.queries {
                    queries_map.insert(
                        String::from_utf8_lossy(k).to_string(),
                        String::from_utf8_lossy(v).to_string(),
                    );
                }

                let mut req_headers_map = HashMap::new();
                for (k, v) in &p.req_headers {
                    req_headers_map.insert(
                        String::from_utf8_lossy(k).to_string(),
                        String::from_utf8_lossy(v).to_string(),
                    );
                }

                let mut rsp_headers_map = HashMap::new();
                for (k, v) in &p.rsp_headers {
                    rsp_headers_map.insert(
                        String::from_utf8_lossy(k).to_string(),
                        String::from_utf8_lossy(v).to_string(),
                    );
                }

                serde_json::json!({
                    "queries": queries_map,
                    "requestHeaders": req_headers_map,
                    "responseHeaders": rsp_headers_map,
                    "body": String::from_utf8_lossy(&p.body),
                    "code": p.code,
                })
            }
            Err(e) => {
                tracing::error!(error = %e, "error while unpack payload");
                // Return null on error
                return serde_json::Value::Null;
            }
        };

        let touched_at_nanos = self.0.touched_at.load(Ordering::Relaxed);
        let updated_at_nanos = self.0.updated_at.load(Ordering::Relaxed);
        let touched_at_secs = touched_at_nanos / 1_000_000_000;
        let updated_at_secs = updated_at_nanos / 1_000_000_000;

        serde_json::json!({
            "key": self.0.key,
            "path": self.0.rule.path.as_deref().unwrap_or(""),
            "payload": payload_map,
            "fingerprintHi": self.0.fingerprint_hi,
            "fingerprintLo": self.0.fingerprint_lo,
            "touchedAt": touched_at_secs,
            "updatedAt": updated_at_secs,
            "refreshQueued": self.0.refresh_queued.load(Ordering::Relaxed),
        })
    }
}
