use std::collections::BTreeSet;

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use knowledge_server::build_test_router;
use serde_json::Value;
use tower::ServiceExt;

const METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

#[tokio::test]
async fn openapi_is_deterministic_and_every_documented_operation_is_routable() {
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
        app.clone()
            .oneshot(
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

    for (path, item) in document["paths"].as_object().unwrap() {
        let uri = concrete_uri(path);
        for method in item
            .as_object()
            .unwrap()
            .keys()
            .filter(|key| METHODS.contains(&key.as_str()))
        {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::from_bytes(method.as_bytes()).unwrap())
                        .uri(&uri)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_ne!(response.status(), StatusCode::NOT_FOUND, "{method} {uri}");
            assert_ne!(
                response.status(),
                StatusCode::METHOD_NOT_ALLOWED,
                "{method} {uri}"
            );
        }
    }
    assert_eq!(
        app.oneshot(
            Request::builder()
                .uri("/api/v1/not-a-real-route")
                .body(Body::empty())
                .unwrap()
        )
        .await
        .unwrap()
        .status(),
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn openapi_models_real_bodies_statuses_headers_and_security() {
    let document = knowledge_server::openapi::document();
    for (path, item) in document["paths"].as_object().unwrap() {
        for (method, operation) in item
            .as_object()
            .unwrap()
            .iter()
            .filter(|(method, _)| METHODS.contains(&method.as_str()))
        {
            for name in path
                .split('/')
                .filter_map(|segment| segment.strip_prefix('{')?.strip_suffix('}'))
            {
                assert!(
                    operation["parameters"]
                        .as_array()
                        .is_some_and(|parameters| parameters.iter().any(|parameter| {
                            parameter["name"] == name
                                && parameter["in"] == "path"
                                && parameter["required"] == true
                        })),
                    "{method} {path} omits required path parameter {name}"
                );
            }
            for (status, response) in operation["responses"].as_object().unwrap() {
                assert_eq!(
                    response["headers"]["X-Request-ID"]["schema"]["format"], "uuid",
                    "{method} {path} response {status} omits X-Request-ID"
                );
                if let Some(content) = response.get("content").and_then(Value::as_object) {
                    for (media, representation) in content {
                        assert!(
                            representation["schema"]
                                .as_object()
                                .is_some_and(|schema| !schema.is_empty()),
                            "{method} {path} response {status} has empty {media} schema"
                        );
                    }
                }
            }
        }
    }

    for operation_id in [
        "login",
        "createInvitation",
        "acceptInvitation",
        "updateUser",
        "createGroup",
        "updateGroup",
        "createSpace",
        "updateSpace",
        "createSpaceGrant",
        "updateSpaceGrant",
        "createPage",
        "updatePage",
        "movePage",
        "restorePageRevision",
        "upsertPagePermission",
        "createExport",
        "updateSettings",
    ] {
        let operation = find_operation(&document, operation_id);
        assert!(
            operation["requestBody"]["content"]["application/json"]["schema"]
                .as_object()
                .is_some_and(|schema| !schema.is_empty()),
            "{operation_id} omits its JSON request schema"
        );
    }

    assert_eq!(
        parameter(&document, "/api/v1/invitations", "post", "Idempotency-Key")["required"],
        true
    );
    assert_eq!(
        parameter(
            &document,
            "/api/v1/pages/{id}/attachments",
            "post",
            "Idempotency-Key"
        )["required"],
        false
    );
    assert_eq!(
        parameter(&document, "/api/v1/exports", "post", "Idempotency-Key")["required"],
        true
    );
    assert_eq!(
        document["paths"]["/api/v1/auth/login"]["post"]["responses"]["200"]["headers"]["Set-Cookie"]
            ["schema"]["type"],
        "string"
    );
    assert!(
        document["paths"]["/api/v1/health/ready"]["get"]["responses"]
            .get("503")
            .is_some()
    );
    assert!(
        document["paths"]["/api/v1/attachments/{id}"]["get"]["responses"]
            .get("206")
            .is_some()
    );
    assert!(
        document["paths"]["/api/v1/attachments/{id}"]["get"]["responses"]
            .get("416")
            .is_some()
    );
    assert_eq!(
        document["paths"]["/api/v1/auth/logout"]["post"]["responses"]["204"].get("content"),
        None
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
    assert!(
        find_operation(&document, "getOpenApi")["responses"]
            .get("401")
            .is_none()
    );
    assert!(
        find_operation(&document, "getSession")["responses"]
            .get("409")
            .is_none()
    );
    assert!(
        find_operation(&document, "getSession")["responses"]
            .get("429")
            .is_none()
    );
    assert!(
        find_operation(&document, "createInvitation")["responses"]
            .get("409")
            .is_some()
    );
    assert!(
        find_operation(&document, "createInvitation")["responses"]
            .get("429")
            .is_some()
    );
    let invitation_expiry = &document["components"]["schemas"]["InvitationCreateRequest"]["properties"]
        ["expiresInHours"];
    assert_eq!(invitation_expiry["type"], "integer");
    assert_eq!(invitation_expiry["minimum"], 1);
    assert_eq!(invitation_expiry["maximum"], 720);
    assert_eq!(invitation_expiry["default"], 168);
    assert_eq!(
        parameter(
            &document,
            "/api/v1/pages/{id}/revisions/{version}",
            "get",
            "version"
        )["schema"]["type"],
        "integer"
    );
    assert_eq!(
        parameter(&document, "/api/v1/attachments/{id}", "get", "Range")["in"],
        "header"
    );
    let upload_size = parameter(&document, "/api/v1/pages/{id}/attachments", "post", "size");
    assert_eq!(upload_size["schema"]["minimum"], 1);
    assert_eq!(upload_size["schema"]["maximum"], 104_857_600);
    assert!(
        find_operation(&document, "movePage")["responses"]
            .get("204")
            .is_some()
    );
    assert!(
        find_operation(&document, "movePage")["responses"]
            .get("200")
            .is_none()
    );
    assert!(
        document["components"]["schemas"]["Group"]["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("memberIds"))
    );
    assert!(
        document["components"]["schemas"]["Page"]["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("stableKey"))
    );
    assert!(
        document["components"]["schemas"]["AuditEventList"]["properties"]
            .get("events")
            .is_some()
    );
    assert!(
        document["components"]["schemas"]["SpaceGrant"]["properties"]
            .get("spaceId")
            .is_none()
    );
    assert_eq!(
        find_operation(&document, "getReadiness")["responses"]["503"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/Readiness"
    );
    assert!(
        find_operation(&document, "login")["responses"]
            .get("401")
            .is_some()
    );
    assert_eq!(
        find_operation(&document, "updateUser")["responses"]["200"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/UserUpdate"
    );
    assert_eq!(
        find_operation(&document, "updateSpaceGrant")["responses"]["200"]["content"]["application/json"]
            ["schema"]["$ref"],
        "#/components/schemas/SpaceGrantUpdate"
    );
    let audit = &document["components"]["schemas"]["AuditEvent"];
    for field in ["actorUserId", "sessionId", "type", "requestId"] {
        assert!(
            audit["properties"].get(field).is_some(),
            "audit omits {field}"
        );
    }
    assert!(audit["properties"].get("eventType").is_none());
    assert!(
        document["components"]["schemas"]["LoginRequest"]["properties"]["password"]
            .get("minLength")
            .is_none()
    );
    for schema in ["GroupUpdateRequest", "SpaceUpdateRequest"] {
        assert_eq!(
            document["components"]["schemas"][schema]["minProperties"],
            1
        );
    }
    for operation_id in [
        "login",
        "acceptInvitation",
        "listGroups",
        "getGroup",
        "getSpace",
        "listPages",
        "getPage",
        "listPageRevisions",
        "getPageRevision",
        "listBacklinks",
        "listAttachments",
        "downloadAttachment",
    ] {
        assert!(
            find_operation(&document, operation_id)["responses"]
                .get("403")
                .is_some(),
            "{operation_id} omits a reachable 403"
        );
    }
    for operation_id in ["updatePage", "restorePageRevision"] {
        assert_eq!(
            find_operation(&document, operation_id)["responses"]["409"]["content"]["application/problem+json"]
                ["schema"]["$ref"],
            "#/components/schemas/VersionConflictProblem"
        );
    }
    let range = &find_operation(&document, "downloadAttachment")["responses"]["416"];
    assert!(range.get("content").is_none());
    assert_eq!(range["headers"]["Content-Range"]["required"], true);
    assert_eq!(
        find_operation(&document, "downloadAttachment")["responses"]["200"]["content"]["*/*"]["schema"]
            ["format"],
        "binary"
    );
    for header_name in [
        "Content-Disposition",
        "Content-Length",
        "Accept-Ranges",
        "Cache-Control",
        "X-Content-Type-Options",
    ] {
        assert_eq!(
            find_operation(&document, "downloadAttachment")["responses"]["200"]["headers"]
                [header_name]["required"],
            true,
            "attachment download omits {header_name}"
        );
    }
    for header_name in ["Content-Disposition", "Content-Length", "Cache-Control"] {
        assert_eq!(
            find_operation(&document, "downloadExport")["responses"]["200"]["headers"][header_name]
                ["required"],
            true,
            "export download omits {header_name}"
        );
    }
}

#[tokio::test]
async fn readiness_503_runtime_shape_matches_its_documented_schema() {
    let response = build_test_router()
        .oneshot(
            Request::builder()
                .uri("/api/v1/health/ready")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let body: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_eq!(
        body.as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<BTreeSet<_>>(),
        [
            "database",
            "masterKey",
            "migrations",
            "objectStorage",
            "status"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
}

fn concrete_uri(path: &str) -> String {
    path.split('/')
        .map(|segment| match segment {
            "{version}" => "1",
            "{token}" => "invalid-token",
            "{subject}" => "user%3A00000000-0000-0000-0000-000000000001",
            value if value.starts_with('{') && value.ends_with('}') => {
                "00000000-0000-0000-0000-000000000001"
            }
            value => value,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn find_operation<'a>(document: &'a Value, id: &str) -> &'a Value {
    document["paths"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|item| item.as_object().unwrap().values())
        .find(|operation| operation["operationId"] == id)
        .unwrap_or_else(|| panic!("missing operation {id}"))
}

fn parameter<'a>(document: &'a Value, path: &str, method: &str, name: &str) -> &'a Value {
    document["paths"][path][method]["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|parameter| parameter["name"] == name)
        .unwrap_or_else(|| panic!("missing {name} parameter on {method} {path}"))
}
