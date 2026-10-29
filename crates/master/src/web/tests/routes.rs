//! The access rule of every route.

use super::*;

/// The access rule of every route. A route added to the router without a line here fails the test below, so that
/// nobody ships an endpoint whose rule was never decided.
#[derive(Clone, Copy, PartialEq)]
enum Rule {
    /// Anyone, even without a login.
    Open,
    /// Every logged-in user, the read-only ones too (what they see is limited by the folders they may read).
    Reader,
    /// Users with a workspace of their own: not the read-only role.
    Workspace,
    /// Administrators only.
    Admin,
}

const ROUTES: &[(&str, &str, Rule)] = &[
    ("POST", "/api/login", Rule::Open),
    ("GET", "/healthz", Rule::Open),
    ("GET", "/api/session", Rule::Open),
    ("POST", "/api/logout", Rule::Reader),
    ("POST", "/api/me/password", Rule::Reader),
    ("GET", "/api/users", Rule::Admin),
    ("POST", "/api/users", Rule::Admin),
    ("PATCH", "/api/users/:name", Rule::Admin),
    ("GET", "/api/settings", Rule::Admin),
    ("PUT", "/api/settings", Rule::Admin),
    ("GET", "/api/mirrors", Rule::Admin),
    ("GET", "/api/mirrors/scope", Rule::Admin),
    ("POST", "/api/mirrors", Rule::Admin),
    ("DELETE", "/api/mirrors/:name", Rule::Admin),
    ("POST", "/api/mirrors/:name/run", Rule::Admin),
    ("POST", "/api/mirrors/:name/toggle", Rule::Admin),
    ("POST", "/api/mirrors/:name/resync", Rule::Admin),
    ("GET", "/api/mirrors/:name/restore-kit", Rule::Admin),
    ("POST", "/api/files/rescan", Rule::Admin),
    ("GET", "/api/plugins", Rule::Admin),
    ("GET", "/api/queue", Rule::Reader),
    ("POST", "/api/queue/pause", Rule::Admin),
    ("POST", "/api/queue/retry", Rule::Admin),
    ("POST", "/api/queue/cancel", Rule::Admin),
    ("POST", "/api/queue/rebuild", Rule::Admin),
    ("GET", "/api/sources", Rule::Workspace),
    ("POST", "/api/sources", Rule::Workspace),
    ("POST", "/api/sources/rclone", Rule::Workspace),
    ("DELETE", "/api/sources/:user/:name", Rule::Workspace),
    ("POST", "/api/sources/:user/:name/run", Rule::Workspace),
    ("POST", "/api/sources/:user/:name/toggle", Rule::Workspace),
    ("POST", "/api/devices", Rule::Workspace),
    ("POST", "/api/shared", Rule::Workspace),
    ("POST", "/api/shared/leave", Rule::Workspace),
    ("GET", "/api/shared/people", Rule::Workspace),
    ("DELETE", "/api/shared/:name", Rule::Workspace),
    ("POST", "/api/shared/:name/members", Rule::Workspace),
    ("DELETE", "/api/shared/:name/members/:user", Rule::Workspace),
    ("POST", "/api/albums", Rule::Workspace),
    ("POST", "/api/files/delete", Rule::Workspace),
    ("POST", "/api/files/move", Rule::Workspace),
    ("POST", "/api/files/restore", Rule::Workspace),
    ("POST", "/api/uploads", Rule::Workspace),
    ("GET", "/api/dashboard", Rule::Reader),
    ("GET", "/api/events", Rule::Reader),
    ("GET", "/api/devices", Rule::Reader),
    ("GET", "/api/shared", Rule::Reader),
    ("GET", "/api/devices/:name", Rule::Reader),
    ("POST", "/api/devices/:name/token", Rule::Reader),
    ("POST", "/api/devices/:name/release", Rule::Reader),
    ("POST", "/api/devices/:name/revoke", Rule::Reader),
    ("GET", "/api/files", Rule::Reader),
    ("GET", "/api/files/info", Rule::Reader),
    ("GET", "/api/files/download", Rule::Reader),
    ("GET", "/api/files/thumb", Rule::Reader),
    ("POST", "/api/files/zip", Rule::Reader),
    ("GET", "/api/media/:id/:variant", Rule::Reader),
    ("GET", "/api/m/:module/:id/*rest", Rule::Reader),
    ("GET", "/api/m/:module/ui/:file", Rule::Reader),
    ("GET", "/api/uploads/:id", Rule::Reader),
    ("PUT", "/api/uploads/:id", Rule::Reader),
    ("DELETE", "/api/uploads/:id", Rule::Reader),
    ("POST", "/api/uploads/:id/finish", Rule::Reader),
    ("GET", "/api/timeline/buckets", Rule::Reader),
    ("GET", "/api/timeline", Rule::Reader),
    ("GET", "/api/library", Rule::Reader),
    ("POST", "/api/library/bulk", Rule::Reader),
    ("GET", "/api/library/:id", Rule::Reader),
    ("PATCH", "/api/library/:id", Rule::Reader),
    ("GET", "/api/library/:id/folder", Rule::Reader),
    ("GET", "/api/tags", Rule::Reader),
    ("DELETE", "/api/tags/:id", Rule::Reader),
    ("GET", "/api/map", Rule::Reader),
    ("GET", "/api/trash", Rule::Reader),
    ("GET", "/api/albums", Rule::Reader),
    ("GET", "/api/albums/:id", Rule::Reader),
    ("PATCH", "/api/albums/:id", Rule::Reader),
    ("DELETE", "/api/albums/:id", Rule::Reader),
    ("POST", "/api/albums/:id/files", Rule::Reader),
    ("POST", "/api/albums/:id/files/remove", Rule::Reader),
    ("POST", "/api/alerts/:id/dismiss", Rule::Reader),
];

#[test]
fn every_route_has_a_rule_and_the_rule_is_kept() {
    // Every route of the router is in the table.
    let source = include_str!("../mod.rs");
    let known: Vec<String> = ROUTES.iter().map(|(m, p, _)| format!("{m} {p}")).collect();
    let mut seen = 0;
    for part in source.split(".route(").skip(1) {
        let path = part.trim_start().trim_start_matches('"');
        let path = &path[..path.find('"').unwrap()];
        let end = part.find("\n    }").unwrap_or(part.len());
        let handlers = &part[..end];
        for verb in ["get", "post", "put", "patch", "delete"] {
            let call = format!("{verb}(");
            let used = handlers
                .match_indices(&call)
                .any(|(i, _)| handlers[..i].chars().last().is_some_and(|c| !c.is_alphanumeric() && c != '_'));
            if used {
                seen += 1;
                let route = format!("{} {path}", verb.to_uppercase());
                assert!(known.contains(&route), "{route} has no access rule in ROUTES");
            }
        }
    }

    assert_eq!(seen, ROUTES.len(), "the table and the router list the same routes");

    let mut api = Api::new();
    block(async {
        let bob = add_user(&mut api, "bob", "normal").await;
        let carol = add_user(&mut api, "carol", "readonly").await;
        let admin = login(&api).await;
        let api = &api;
        for (method, uri, rule) in ROUTES {
            if *rule == Rule::Open {
                continue;
            }
            let uri = uri
                .replace(":name", "x")
                .replace(":user", "x")
                .replace(":id", "AAAAAAAAAAAAAAAAAAAAAA")
                .replace(":variant", "thumb")
                .replace(":file", "index.js")
                .replace("*rest", "a");
            let status = |cookie: String| {
                let uri = uri.clone();
                async move { api.cookie_for(&cookie, Method::from_bytes(method.as_bytes()).unwrap(), uri).await }
            };
            assert_eq!(status(String::new()).await, StatusCode::UNAUTHORIZED, "{method} {uri} without a login");
            if uri == "/api/logout" {
                continue; // it would end the session the next lines use
            }
            let must_refuse: &[(&str, &String)] = match rule {
                Rule::Admin => &[("normal", &bob), ("readonly", &carol)],
                Rule::Workspace => &[("readonly", &carol)],
                _ => &[],
            };
            for (who, cookie) in must_refuse {
                let got = status((*cookie).clone()).await;
                // A request body that does not parse is turned away before the handler (and its rule) runs, which is
                // as good as a refusal; the tests above send valid requests and see the 403.
                let turned_away = *method != "GET" && matches!(got.as_u16(), 400 | 404 | 422);
                assert!(got == StatusCode::FORBIDDEN || turned_away, "{method} {uri} as {who}: {got}");
            }
            // The role that is allowed gets past the rule, whatever the handler then says about the request.
            let allowed = if *rule == Rule::Admin { &admin } else { &bob };
            assert_ne!(status(allowed.clone()).await, StatusCode::FORBIDDEN, "{method} {uri} as its own role");
        }
    });
}
