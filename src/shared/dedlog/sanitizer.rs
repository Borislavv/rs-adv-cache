//! Log message sanitization for deduplicated logging.
//

use regex::Regex;

/// Sanitization rule with regex pattern and placeholder
struct Rule {
    re: Regex,
    placeholder: &'static str,
}

/// Sanitizer applies an ordered set of sanitization rules
pub struct Sanitizer {
    rules: Vec<Rule>,
    collapse_spaces: bool,
}

/// Options for customizing Sanitizer
pub struct WithCollapseSpaces(pub bool);

impl Sanitizer {
    /// Creates a new log sanitizer with conservative, production-ready rules
    pub fn new(opts: WithCollapseSpaces) -> Self {
        let rules = vec![
            // RFC3339 timestamps
            Rule {
                re: Regex::new(r"\b\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d{1,9})?(?:Z|[+-]\d{2}:\d{2})\b").unwrap(),
                placeholder: "<ts>",
            },
            // Common log date: 10/Oct/2000:13:55:36 -0700
            Rule {
                re: Regex::new(r"\b\d{2}/[A-Za-z]{3}/\d{4}:\d{2}:\d{2}:\d{2} [+-]\d{4}\b").unwrap(),
                placeholder: "<ts>",
            },
            // Unix epoch seconds
            Rule {
                re: Regex::new(r"\b1[5-9]\d{8}\b").unwrap(),
                placeholder: "<unix>",
            },
            // Unix epoch milliseconds
            Rule {
                re: Regex::new(r"\b1[5-9]\d{11,12}\b").unwrap(),
                placeholder: "<unixms>",
            },
            // UUID v1-5
            Rule {
                re: Regex::new(r"\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}\b").unwrap(),
                placeholder: "<uuid>",
            },
            // IPv4
            Rule {
                re: Regex::new(r"\b(?:\d{1,3}\.){3}\d{1,3}\b").unwrap(),
                placeholder: "<ip4>",
            },
            // IPv6 (simplified)
            Rule {
                re: Regex::new(r"\b(?:[A-Fa-f0-9]{1,4}:){2,7}[A-Fa-f0-9]{1,4}\b|::1\b").unwrap(),
                placeholder: "<ip6>",
            },
            // Hostnames
            Rule {
                re: Regex::new(r"\b(?:[A-Za-z0-9-]{1,63}\.)+[A-Za-z]{2,}\b").unwrap(),
                placeholder: "<host>",
            },
            // Emails
            Rule {
                re: Regex::new(r"\b[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}\b").unwrap(),
                placeholder: "<email>",
            },
            // URLs
            Rule {
                re: Regex::new(r"\bhttps?://[^\s]+").unwrap(),
                placeholder: "<url>",
            },
            // MAC addresses
            Rule {
                re: Regex::new(r"\b(?:[0-9A-Fa-f]{2}[:-]){5}[0-9A-Fa-f]{2}\b").unwrap(),
                placeholder: "<mac>",
            },
            // Long hex-like IDs (16-64 hex chars)
            Rule {
                re: Regex::new(r"\b[0-9a-fA-F]{16,64}\b").unwrap(),
                placeholder: "<hex>",
            },
        ];

        Self {
            rules,
            collapse_spaces: opts.0,
        }
    }

    /// Sanitizes a message string according to the configured rules
    pub fn sanitize(&self, err: &str) -> String {
        if err.is_empty() {
            return err.to_string();
        }

        let mut result = err.to_string();
        for rule in &self.rules {
            result = rule.re.replace_all(&result, rule.placeholder).to_string();
        }

        if self.collapse_spaces {
            // Collapse multiple whitespace into single spaces
            result = result.split_whitespace().collect::<Vec<_>>().join(" ");
        }

        result
    }
}
