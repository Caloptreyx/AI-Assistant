const PATH_CHARS: &[u8] = b"/-._~%!$&'()*+,;=:@";

/// The decoded, lower-cased segments of a path, so encoding or case tricks cannot hide one.
pub fn segments(path: &str) -> Vec<String> {
    path.split(['?', '#'])
        .next()
        .unwrap_or_default()
        .split('/')
        .filter(|segment| !segment.is_empty())
        .map(|segment| crate::access::percent_decode(segment).to_ascii_lowercase())
        .collect()
}

/// This extension's own endpoints: a tool must never start another chat turn or confirm an
/// action by itself.
fn targets_this_extension(segments: &[String]) -> bool {
    segments.iter().any(|segment| segment == crate::PACKAGE)
        || (segments.len() > 4
            && segments[..3] == ["api", "client", "servers"]
            && segments[4] == crate::SERVER_ROUTE)
}

pub fn check(path: &str) -> Result<(), String> {
    if !(path.starts_with("/api/client/") || path.starts_with("/api/admin/")) {
        return Err(format!(
            "Path must start with /api/client/ or /api/admin/, got \"{path}\""
        ));
    }

    for segment in path[1..].split('/') {
        if segment.is_empty() {
            return Err("Path contains an empty segment".into());
        }
        // Percent-encoding must not hide a dot segment or another separator.
        let decoded = crate::access::percent_decode(segment);
        if decoded == "." || decoded == ".." {
            return Err("Path must not contain . or .. segments".into());
        }
        if decoded.contains(['/', '\\', '?', '#']) {
            return Err(format!(
                "Path segment \"{segment}\" must not encode a slash, backslash, ? or #"
            ));
        }
        if decoded.contains(['{', '}']) {
            return Err(format!(
                "Replace the placeholder \"{segment}\" in the path with a real value"
            ));
        }
    }

    if targets_this_extension(&segments(path)) {
        return Err("The assistant cannot call its own endpoints".into());
    }

    if let Some(bad) = path.chars().find(|c| {
        !(c.is_ascii_alphanumeric() || (c.is_ascii() && PATH_CHARS.contains(&(*c as u8))))
    }) {
        return Err(format!(
            "Path contains the character {bad:?}; send query parameters through `query`"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check;

    #[test]
    fn accepts_ordinary_panel_paths() {
        assert!(check("/api/client/servers/0a1b2c3d/files/list").is_ok());
        assert!(check("/api/admin/users/5f0e").is_ok());
        assert!(check("/api/client/servers/0a1b2c3d/backups/My%20Backup").is_ok());
    }

    #[test]
    fn refuses_traversal_and_encoded_separators() {
        for path in [
            "/api/client/../admin/users",
            "/api/client/./servers",
            "/api/client/%2e%2e/admin/users",
            "/api/client/%2E%2E/admin/users",
            "/api/client/.%2e/admin/users",
            "/api/client/servers%2f..%2f..%2fadmin/users",
            "/api/client/servers%5Cadmin",
            "/api/client/servers%3Fx=1",
            "/api/client//servers",
            "/api/client/servers/%7Bserver%7D",
            "/api/Admin/users",
            "/api/%61dmin/users",
            "/api/client/servers?x=1",
        ] {
            assert!(check(path).is_err(), "{path}");
        }
    }

    #[test]
    fn cannot_reach_its_own_routes_even_encoded() {
        for path in [
            "/api/admin/extensions/dev.caloptreyx.aiassistant/settings",
            "/api/admin/extensions/dev%2Ecaloptreyx.aiassistant/conversations",
            "/api/client/servers/0a1b2c3d/ai-assistant/conversations",
            "/api/client/servers/0a1b2c3d/AI-Assistant/conversations/x/actions/y/confirm",
            "/api/client/servers/0a1b2c3d/ai%2Dassistant/conversations",
        ] {
            assert!(check(path).is_err(), "{path}");
        }
    }
}
