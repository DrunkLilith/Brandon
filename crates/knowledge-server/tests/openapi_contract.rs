use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
};

use axum::{
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use knowledge_server::build_test_router;
use serde_json::Value;
use tower::ServiceExt;

const METHODS: [&str; 7] = ["get", "post", "put", "patch", "delete", "head", "options"];

#[tokio::test]
async fn openapi_is_deterministic_and_matches_runtime_route_sources() {
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

    let documented: BTreeMap<String, BTreeSet<String>> = document["paths"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(path, item)| {
            (
                path.clone(),
                item.as_object()
                    .unwrap()
                    .keys()
                    .filter(|key| METHODS.contains(&key.as_str()))
                    .cloned()
                    .collect(),
            )
        })
        .collect();
    assert_eq!(documented, registered_operations());
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
}

fn registered_operations() -> BTreeMap<String, BTreeSet<String>> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut sources = vec![fs::read_to_string(root.join("lib.rs")).unwrap()];
    for entry in fs::read_dir(root.join("routes")).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "rs") {
            sources.push(fs::read_to_string(path).unwrap());
        }
    }
    let mut result: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for source in sources {
        let mut offset = 0;
        while let Some(relative) = source[offset..].find(".route(") {
            let start = offset + relative;
            let open = start + ".route".len();
            let mut depth = 0_i32;
            let mut end = open;
            for (index, byte) in source.as_bytes()[open..].iter().enumerate() {
                match byte {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            end = open + index + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
            let registration = &source[start..end];
            let first_quote = registration.find('"').unwrap();
            let after_quote = &registration[first_quote + 1..];
            let path_end = after_quote.find('"').unwrap();
            let path = &after_quote[..path_end];
            if path.starts_with("/api/v1/") {
                let methods = result.entry(path.to_owned()).or_default();
                for method in METHODS {
                    if registration.contains(&format!("{method}(")) {
                        methods.insert(method.to_owned());
                    }
                }
            }
            offset = end;
        }
    }
    result
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
