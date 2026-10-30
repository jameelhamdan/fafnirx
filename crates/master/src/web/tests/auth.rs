//! Logging in, sessions and the headers every response carries.

use super::*;

#[test]
fn login_is_required_and_wrong_passwords_are_refused() {
    let mut api = Api::new();
    block(async {
        assert_eq!(api.get("/api/dashboard").await.0, StatusCode::UNAUTHORIZED);
        assert_eq!(api.get("/api/session").await.1["user"], Value::Null);
        assert_eq!(
            api.json(Method::POST, "/api/login", json!({"username": "admin", "password": "nope"})).await.0,
            StatusCode::UNAUTHORIZED
        );

        let (status, headers, _) = api
            .send(
                Method::POST,
                "/api/login",
                Body::from(json!({"username": "admin", "password": "secret"}).to_string()),
                Some("application/json"),
                &[],
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        api.cookie = headers[header::SET_COOKIE].to_str().unwrap().split(';').next().unwrap().to_string();
        assert_eq!(api.get("/api/session").await.1["user"]["role"], "admin");
        let (status, dash) = api.get("/api/dashboard").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(dash["totals"]["files"], 0);

        // A changing call without the marker header is refused even with a valid cookie.
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/devices")
            .header(header::COOKIE, &api.cookie)
            .body(Body::empty())
            .unwrap();
        assert_eq!(api.app.clone().oneshot(req).await.unwrap().status(), StatusCode::FORBIDDEN);

        // Logging out ends the session in the database, so the cookie is no good afterwards.
        let (status, _, _) = api.send(Method::POST, "/api/logout", Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(api.get("/api/dashboard").await.0, StatusCode::UNAUTHORIZED);

        // Wrong guesses are counted per user name as well as per address, and the right password is refused meanwhile.
        for _ in 0..5 {
            assert_eq!(login_as(&api, "bob", "wrong").await, "");
        }
        let blocked = api.json(Method::POST, "/api/login", json!({"username": "bob", "password": "wrong"})).await;
        assert_eq!(blocked.0, StatusCode::TOO_MANY_REQUESTS);
    });
}

#[test]
fn every_response_carries_the_security_headers() {
    let api = Api::new();
    block(async {
        let (status, page, _) = api.send(Method::GET, "/healthz", Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::OK, "the health check needs no login");
        let csp = page[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert!(csp.contains("script-src 'self'") && csp.contains("frame-ancestors 'none'"), "{csp}");
        assert_eq!(page[header::X_FRAME_OPTIONS], "DENY");
        assert_eq!(page[header::X_CONTENT_TYPE_OPTIONS], "nosniff");

        let (_, app_page, _) = api.send(Method::GET, "/timeline", Body::empty(), None, &[]).await;
        assert!(
            app_page[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("script-src 'self'"),
            "also on the web app pages"
        );
        assert!(page.get(header::STRICT_TRANSPORT_SECURITY).is_none(), "plain http: no HSTS");

        let (status, refused, _) = api.send(Method::GET, "/api/dashboard", Body::empty(), None, &[]).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(refused[header::X_FRAME_OPTIONS], "DENY", "also on refusals");
        let (_, behind_tls, _) =
            api.send(Method::GET, "/healthz", Body::empty(), None, &[("x-forwarded-proto", "https")]).await;
        assert!(behind_tls.get(header::STRICT_TRANSPORT_SECURITY).is_some());
    });
}
