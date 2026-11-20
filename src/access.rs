use crate::proxy::path::segments;
use axum::http::Method;

/// What the panel requests of one tool run may do. Every request a tool makes passes
/// [`Access::check_request`] before it reaches the panel, and the panel then applies the user's
/// own permissions on top.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Access {
    /// Only true for an action the user confirmed. Without it only GET requests go through.
    pub confirmed: bool,
    /// Server chats: requests must stay inside this server's client API.
    pub server: Option<String>,
}

impl Access {
    pub fn check_request(&self, method: &Method, path: &str) -> Result<(), String> {
        if !self.confirmed && method != Method::GET {
            return Err(format!(
                "{method} requests change something, so they only run after the user confirms them."
            ));
        }

        if !self.confirmed && issues_credentials(path) {
            return Err(
                "Endpoints that hand out console, upload or node credentials (or change two-factor setup) only run after the user confirms them.".into(),
            );
        }

        if let Some(server) = &self.server {
            let segments = segments(path);
            let inside = segments.len() >= 4
                && segments[..3] == ["api", "client", "servers"]
                && segments[3] == server.to_ascii_lowercase();
            if !inside {
                return Err(format!(
                    "This chat belongs to server {server}, so it can only reach that server's endpoints."
                ));
            }
        }

        Ok(())
    }
}

/// GET endpoints that hand out credentials which can change things outside the panel API (a
/// Wings websocket token with console and power rights, a signed upload URL, node and database
/// agent tokens), or that write state despite being GET (two-factor setup stores a new secret).
/// `*` stands for one path segment.
const CREDENTIAL_ENDPOINTS: &[&[&str]] = &[
    &["api", "client", "servers", "*", "websocket"],
    &["api", "client", "servers", "*", "files", "upload"],
    &["api", "client", "account", "two-factor"],
    &["api", "admin", "nodes", "*", "token"],
    &["api", "admin", "nodes", "*", "config"],
    &["api", "admin", "database-agent-hosts", "*", "token"],
    &["api", "admin", "database-agent-hosts", "*", "config"],
];

pub(crate) fn percent_decode(segment: &str) -> String {
    let bytes = segment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let decoded = (bytes[index] == b'%')
            .then(|| segment.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match decoded {
            Some(byte) => {
                out.push(byte);
                index += 3;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn issues_credentials(path: &str) -> bool {
    let segments = segments(path);
    CREDENTIAL_ENDPOINTS.iter().any(|pattern| {
        pattern.len() == segments.len()
            && pattern
                .iter()
                .zip(&segments)
                .all(|(expected, actual)| *expected == "*" || expected == actual)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unconfirmed(server: Option<&str>) -> Access {
        Access {
            confirmed: false,
            server: server.map(str::to_string),
        }
    }

    fn confirmed(server: Option<&str>) -> Access {
        Access {
            confirmed: true,
            server: server.map(str::to_string),
        }
    }

    #[test]
    fn without_confirmation_only_get_requests_pass() {
        let access = unconfirmed(None);
        assert!(access.check_request(&Method::GET, "/api/admin/users").is_ok());
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            let err = access
                .check_request(&method, "/api/client/servers/abc/power")
                .unwrap_err();
            assert!(err.contains("confirm"), "{err}");
            assert!(
                confirmed(None)
                    .check_request(&method, "/api/client/servers/abc/power")
                    .is_ok()
            );
        }
    }

    #[test]
    fn credential_gets_need_confirmation() {
        for path in [
            "/api/client/servers/0a1b2c3d/websocket",
            "/api/client/servers/0a1b2c3d/files/%75pload",
            "/api/client/servers/0a1b2c3d/WebSocket",
            "/api/client/account/two-factor",
            "/api/admin/nodes/5f0e/token",
            "/api/admin/database-agent-hosts/5f0e/config",
        ] {
            assert!(
                unconfirmed(None).check_request(&Method::GET, path).is_err(),
                "{path}"
            );
            assert!(confirmed(None).check_request(&Method::GET, path).is_ok(), "{path}");
        }
        assert!(
            unconfirmed(None)
                .check_request(&Method::GET, "/api/client/servers/0a1b2c3d/files/list")
                .is_ok()
        );
    }

    #[test]
    fn a_server_chat_stays_on_its_server() {
        let server = "123e4567-e89b-12d3-a456-426614174000";
        for access in [unconfirmed(Some(server)), confirmed(Some(server))] {
            assert!(
                access
                    .check_request(&Method::GET, &format!("/api/client/servers/{server}/logs"))
                    .is_ok()
            );
            for path in [
                "/api/client/servers/0a1b2c3d/logs".to_string(),
                "/api/client/servers".to_string(),
                "/api/admin/servers".to_string(),
                "/api/client/account".to_string(),
                format!("/api/client/servers%2F{server}/logs"),
            ] {
                assert!(access.check_request(&Method::GET, &path).is_err(), "{path}");
            }
        }
    }
}
