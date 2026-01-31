use crate::types::{DepsArgs, NodeInfo};
// ============================================================================
// Helpers
// ============================================================================

pub fn format_node_label(info: &NodeInfo, args: &DepsArgs) -> String {
    let sanitized = sanitize_name(&info.name);
    if args.show_versions {
        format!("{}_{}", sanitized, info.version.replace('.', "_"))
    } else {
        sanitized
    }
}

pub fn sanitize_name(name: &str) -> String {
    name.replace('-', "_").replace('.', "_")
}

/// Check if name matches any pattern in the list (supports * wildcard)
pub fn matches_any_pattern(name: &str, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| matches_pattern(name, pattern))
}

/// Simple wildcard pattern matching (* matches any sequence of characters)
fn matches_pattern(name: &str, pattern: &str) -> bool {
    if !pattern.contains('*') {
        return name == pattern;
    }

    let parts: Vec<&str> = pattern.split('*').collect();

    if parts.len() == 2 {
        // Single wildcard
        let (prefix, suffix) = (parts[0], parts[1]);
        return name.starts_with(prefix) && name.ends_with(suffix)
            && name.len() >= prefix.len() + suffix.len();
    }

    // Multiple wildcards - use regex-like matching
    let mut pos = 0;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        if let Some(found_pos) = name[pos..].find(part) {
            if i == 0 && found_pos != 0 {
                return false; // First part must be at start
            }
            pos += found_pos + part.len();
        } else {
            return false;
        }
    }

    // If pattern ends with *, we're done; otherwise check we consumed all
    pattern.ends_with('*') || pos == name.len()
}
