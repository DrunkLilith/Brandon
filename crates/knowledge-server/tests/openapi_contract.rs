use std::collections::{BTreeMap, BTreeSet};

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use knowledge_server::build_test_router;
use serde_json::Value;
use tower::ServiceExt;

#[tokio::test]
async fn openapi_is_deterministic_complete_and_models_security_and_problems() {
    let app = build_test_router();
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "public, max-age=3600"
    );
    let first = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let second = to_bytes(
        app.oneshot(
            Request::builder()
                .uri("/api/v1/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .into_body(),
        usize::MAX,
    )
    .await
    .unwrap();
    assert_eq!(first, second);
    let document: Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(document["openapi"], "3.1.0");
    let actual: BTreeSet<&str> = document["paths"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    let expected: BTreeSet<&str> = [
        "/api/v1/admin/settings",
        "/api/v1/admin/status",
        "/api/v1/attachments/{id}",
        "/api/v1/attachments/{id}/unarchive",
        "/api/v1/audit-events",
        "/api/v1/auth/csrf",
        "/api/v1/auth/login",
        "/api/v1/auth/logout",
        "/api/v1/auth/session",
        "/api/v1/exports",
        "/api/v1/exports/{id}/download",
        "/api/v1/groups",
        "/api/v1/groups/archived",
        "/api/v1/groups/{group_id}/members/{user_id}",
        "/api/v1/groups/{id}",
        "/api/v1/groups/{id}/restore",
        "/api/v1/health/live",
        "/api/v1/health/ready",
        "/api/v1/invitations",
        "/api/v1/invitations/{id}",
        "/api/v1/invitations/{token}/accept",
        "/api/v1/pages",
        "/api/v1/pages/{id}",
        "/api/v1/pages/{id}/attachments",
        "/api/v1/pages/{id}/backlinks",
        "/api/v1/pages/{id}/move",
        "/api/v1/pages/{id}/permissions/{subject}",
        "/api/v1/pages/{id}/restore",
        "/api/v1/pages/{id}/revisions",
        "/api/v1/pages/{id}/revisions/{version}",
        "/api/v1/pages/{id}/unarchive",
        "/api/v1/search",
        "/api/v1/spaces",
        "/api/v1/spaces/archived",
        "/api/v1/spaces/{id}",
        "/api/v1/spaces/{id}/restore",
        "/api/v1/spaces/{id}/subjects",
        "/api/v1/spaces/{space_id}/grants",
        "/api/v1/spaces/{space_id}/grants/{grant_id}",
        "/api/v1/users",
        "/api/v1/users/{id}",
        "/api/v1/users/{id}/revoke-sessions",
    ]
    .into_iter()
    .collect();
    assert_eq!(actual, expected);
    let actual_operations: BTreeMap<&str, BTreeSet<&str>> = document["paths"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(path, item)| {
            (
                path.as_str(),
                item.as_object()
                    .unwrap()
                    .keys()
                    .map(String::as_str)
                    .collect(),
            )
        })
        .collect();
    let expected_operations: BTreeMap<&str, BTreeSet<&str>> = [
        ("/api/v1/admin/settings", &["get", "patch"][..]),
        ("/api/v1/admin/status", &["get"]),
        ("/api/v1/attachments/{id}", &["delete", "get"]),
        ("/api/v1/attachments/{id}/unarchive", &["post"]),
        ("/api/v1/audit-events", &["get"]),
        ("/api/v1/auth/csrf", &["get"]),
        ("/api/v1/auth/login", &["post"]),
        ("/api/v1/auth/logout", &["post"]),
        ("/api/v1/auth/session", &["get"]),
        ("/api/v1/exports", &["get", "post"]),
        ("/api/v1/exports/{id}/download", &["get"]),
        ("/api/v1/groups", &["get", "post"]),
        ("/api/v1/groups/archived", &["get"]),
        (
            "/api/v1/groups/{group_id}/members/{user_id}",
            &["delete", "put"],
        ),
        ("/api/v1/groups/{id}", &["delete", "get", "patch"]),
        ("/api/v1/groups/{id}/restore", &["post"]),
        ("/api/v1/health/live", &["get"]),
        ("/api/v1/health/ready", &["get"]),
        ("/api/v1/invitations", &["get", "post"]),
        ("/api/v1/invitations/{id}", &["delete"]),
        ("/api/v1/invitations/{token}/accept", &["post"]),
        ("/api/v1/pages", &["get", "post"]),
        ("/api/v1/pages/{id}", &["delete", "get", "patch"]),
        ("/api/v1/pages/{id}/attachments", &["get", "post"]),
        ("/api/v1/pages/{id}/backlinks", &["get"]),
        ("/api/v1/pages/{id}/move", &["post"]),
        (
            "/api/v1/pages/{id}/permissions/{subject}",
            &["delete", "put"],
        ),
        ("/api/v1/pages/{id}/restore", &["post"]),
        ("/api/v1/pages/{id}/revisions", &["get"]),
        ("/api/v1/pages/{id}/revisions/{version}", &["get"]),
        ("/api/v1/pages/{id}/unarchive", &["post"]),
        ("/api/v1/search", &["get"]),
        ("/api/v1/spaces", &["get", "post"]),
        ("/api/v1/spaces/archived", &["get"]),
        ("/api/v1/spaces/{id}", &["delete", "get", "patch"]),
        ("/api/v1/spaces/{id}/restore", &["post"]),
        ("/api/v1/spaces/{id}/subjects", &["get"]),
        ("/api/v1/spaces/{space_id}/grants", &["get", "post"]),
        (
            "/api/v1/spaces/{space_id}/grants/{grant_id}",
            &["delete", "patch"],
        ),
        ("/api/v1/users", &["get"]),
        ("/api/v1/users/{id}", &["patch"]),
        ("/api/v1/users/{id}/revoke-sessions", &["post"]),
    ]
    .into_iter()
    .map(|(path, methods)| (path, methods.iter().copied().collect()))
    .collect();
    assert_eq!(actual_operations, expected_operations);
    for (path, item) in document["paths"].as_object().unwrap() {
        for name in path
            .split('/')
            .filter_map(|segment| segment.strip_prefix('{')?.strip_suffix('}'))
        {
            for operation in item.as_object().unwrap().values() {
                assert!(
                    operation["parameters"]
                        .as_array()
                        .is_some_and(|parameters| {
                            parameters.iter().any(|parameter| {
                                parameter["name"] == name
                                    && parameter["in"] == "path"
                                    && parameter["required"] == true
                            })
                        }),
                    "{path} omits required path parameter {name}"
                );
            }
        }
    }
    for (path, method, name) in [
        ("/api/v1/audit-events", "get", "after"),
        ("/api/v1/audit-events", "get", "limit"),
        ("/api/v1/exports", "get", "after"),
        ("/api/v1/exports", "get", "limit"),
        ("/api/v1/pages/{id}/attachments", "get", "after"),
        ("/api/v1/pages/{id}/attachments", "get", "archived"),
    ] {
        assert!(
            document["paths"][path][method]["parameters"]
                .as_array()
                .unwrap()
                .iter()
                .any(|parameter| parameter["name"] == name && parameter["in"] == "query"),
            "{method} {path} omits query parameter {name}"
        );
    }
    assert!(
        document["paths"]["/api/v1/auth/logout"]["post"]["responses"]["204"]
            .get("content")
            .is_none()
    );
    assert_eq!(
        document["components"]["securitySchemes"]["sessionCookie"]["in"],
        "cookie"
    );
    assert_eq!(
        document["components"]["securitySchemes"]["csrfToken"]["name"],
        "X-CSRF-Token"
    );
    assert_eq!(
        document["components"]["schemas"]["Problem"]["required"],
        serde_json::json!(["type", "title", "status", "detail", "code"])
    );
    assert_eq!(
        document["paths"]["/api/v1/auth/login"]["post"]["responses"]["401"]["content"]["application/problem+json"]
            ["schema"]["$ref"],
        "#/components/schemas/Problem"
    );
}
