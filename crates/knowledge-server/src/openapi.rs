//! Deterministic OpenAPI 3.1 contract shared by HTTP serving and client generation.

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

#[derive(Clone, Copy)]
enum Access {
    Public,
    Session,
    Mutation,
}

fn operation(id: &str, access: Access, success: u16) -> Value {
    let mut value = json!({
        "operationId": id,
        "responses": {
            success.to_string(): {"description":"Success","content":{"application/json":{"schema":{}}}},
            "401": problem("Authentication required"),
            "403": problem("Forbidden"),
            "404": problem("Not found"),
            "422": problem("Validation failed"),
            "500": problem("Internal error")
        }
    });
    if success == 204 {
        value["responses"][success.to_string()]
            .as_object_mut()
            .expect("success response is an object")
            .remove("content");
    }
    value["security"] = match access {
        Access::Public => json!([]),
        Access::Session => json!([{"sessionCookie":[]}]),
        Access::Mutation => json!([{"sessionCookie":[],"csrfToken":[]}]),
    };
    value
}

fn problem(description: &str) -> Value {
    json!({"description":description,"content":{"application/problem+json":{"schema":{"$ref":"#/components/schemas/Problem"}}}})
}

fn add(paths: &mut BTreeMap<String, Value>, path: &str, operations: &[(&str, &str, Access, u16)]) {
    let mut item = Map::new();
    for (method, id, access, success) in operations {
        let mut value = operation(id, *access, *success);
        let parameters: Vec<Value> = path
            .split('/')
            .filter_map(|segment| segment.strip_prefix('{')?.strip_suffix('}'))
            .map(|name| json!({"name":name,"in":"path","required":true,"schema":{"type":"string"}}))
            .collect();
        if !parameters.is_empty() {
            value["parameters"] = Value::Array(parameters);
        }
        item.insert((*method).to_owned(), value);
    }
    paths.insert(path.to_owned(), Value::Object(item));
}

fn parameter(
    paths: &mut BTreeMap<String, Value>,
    path: &str,
    method: &str,
    name: &str,
    location: &str,
    required: bool,
    schema: Value,
) {
    let parameters = &mut paths.get_mut(path).expect("known path")[method]["parameters"];
    if parameters.is_null() {
        *parameters = json!([]);
    }
    parameters
        .as_array_mut()
        .expect("parameters are an array")
        .push(json!({"name":name,"in":location,"required":required,"schema":schema}));
}

/// Builds the single environment-independent API document.
pub fn document() -> Value {
    let mut paths = BTreeMap::new();
    add(
        &mut paths,
        "/api/v1/health/live",
        &[("get", "getLiveness", Access::Public, 200)],
    );
    add(
        &mut paths,
        "/api/v1/health/ready",
        &[("get", "getReadiness", Access::Public, 200)],
    );
    add(
        &mut paths,
        "/api/v1/auth/login",
        &[("post", "login", Access::Public, 200)],
    );
    add(
        &mut paths,
        "/api/v1/auth/logout",
        &[("post", "logout", Access::Mutation, 204)],
    );
    add(
        &mut paths,
        "/api/v1/auth/session",
        &[("get", "getSession", Access::Session, 200)],
    );
    add(
        &mut paths,
        "/api/v1/auth/csrf",
        &[("get", "getCsrf", Access::Session, 200)],
    );
    add(
        &mut paths,
        "/api/v1/invitations",
        &[
            ("get", "listInvitations", Access::Session, 200),
            ("post", "createInvitation", Access::Mutation, 201),
        ],
    );
    add(
        &mut paths,
        "/api/v1/invitations/{id}",
        &[("delete", "revokeInvitation", Access::Mutation, 204)],
    );
    add(
        &mut paths,
        "/api/v1/invitations/{token}/accept",
        &[("post", "acceptInvitation", Access::Public, 201)],
    );
    add(
        &mut paths,
        "/api/v1/users",
        &[("get", "listUsers", Access::Session, 200)],
    );
    add(
        &mut paths,
        "/api/v1/users/{id}",
        &[("patch", "updateUser", Access::Mutation, 200)],
    );
    add(
        &mut paths,
        "/api/v1/users/{id}/revoke-sessions",
        &[("post", "revokeUserSessions", Access::Mutation, 204)],
    );
    add(
        &mut paths,
        "/api/v1/groups",
        &[
            ("get", "listGroups", Access::Session, 200),
            ("post", "createGroup", Access::Mutation, 201),
        ],
    );
    add(
        &mut paths,
        "/api/v1/groups/archived",
        &[("get", "listArchivedGroups", Access::Session, 200)],
    );
    add(
        &mut paths,
        "/api/v1/groups/{id}",
        &[
            ("get", "getGroup", Access::Session, 200),
            ("patch", "updateGroup", Access::Mutation, 200),
            ("delete", "archiveGroup", Access::Mutation, 204),
        ],
    );
    add(
        &mut paths,
        "/api/v1/groups/{id}/restore",
        &[("post", "restoreGroup", Access::Mutation, 204)],
    );
    add(
        &mut paths,
        "/api/v1/groups/{group_id}/members/{user_id}",
        &[
            ("put", "addGroupMember", Access::Mutation, 204),
            ("delete", "removeGroupMember", Access::Mutation, 204),
        ],
    );
    add(
        &mut paths,
        "/api/v1/spaces",
        &[
            ("get", "listSpaces", Access::Session, 200),
            ("post", "createSpace", Access::Mutation, 201),
        ],
    );
    add(
        &mut paths,
        "/api/v1/spaces/archived",
        &[("get", "listArchivedSpaces", Access::Session, 200)],
    );
    add(
        &mut paths,
        "/api/v1/spaces/{id}",
        &[
            ("get", "getSpace", Access::Session, 200),
            ("patch", "updateSpace", Access::Mutation, 200),
            ("delete", "archiveSpace", Access::Mutation, 204),
        ],
    );
    add(
        &mut paths,
        "/api/v1/spaces/{id}/restore",
        &[("post", "restoreSpace", Access::Mutation, 204)],
    );
    add(
        &mut paths,
        "/api/v1/spaces/{id}/subjects",
        &[("get", "listSubjects", Access::Session, 200)],
    );
    add(
        &mut paths,
        "/api/v1/spaces/{space_id}/grants",
        &[
            ("get", "listSpaceGrants", Access::Session, 200),
            ("post", "createSpaceGrant", Access::Mutation, 201),
        ],
    );
    add(
        &mut paths,
        "/api/v1/spaces/{space_id}/grants/{grant_id}",
        &[
            ("patch", "updateSpaceGrant", Access::Mutation, 200),
            ("delete", "deleteSpaceGrant", Access::Mutation, 204),
        ],
    );
    add(
        &mut paths,
        "/api/v1/pages",
        &[
            ("get", "listPages", Access::Session, 200),
            ("post", "createPage", Access::Mutation, 201),
        ],
    );
    add(
        &mut paths,
        "/api/v1/pages/{id}",
        &[
            ("get", "getPage", Access::Session, 200),
            ("patch", "updatePage", Access::Mutation, 200),
            ("delete", "archivePage", Access::Mutation, 204),
        ],
    );
    add(
        &mut paths,
        "/api/v1/pages/{id}/move",
        &[("post", "movePage", Access::Mutation, 200)],
    );
    add(
        &mut paths,
        "/api/v1/pages/{id}/unarchive",
        &[("post", "unarchivePage", Access::Mutation, 204)],
    );
    add(
        &mut paths,
        "/api/v1/pages/{id}/revisions",
        &[("get", "listPageRevisions", Access::Session, 200)],
    );
    add(
        &mut paths,
        "/api/v1/pages/{id}/revisions/{version}",
        &[("get", "getPageRevision", Access::Session, 200)],
    );
    add(
        &mut paths,
        "/api/v1/pages/{id}/restore",
        &[("post", "restorePageRevision", Access::Mutation, 200)],
    );
    add(
        &mut paths,
        "/api/v1/pages/{id}/backlinks",
        &[("get", "listBacklinks", Access::Session, 200)],
    );
    add(
        &mut paths,
        "/api/v1/pages/{id}/permissions/{subject}",
        &[
            ("put", "upsertPagePermission", Access::Mutation, 200),
            ("delete", "deletePagePermission", Access::Mutation, 204),
        ],
    );
    add(
        &mut paths,
        "/api/v1/pages/{id}/attachments",
        &[
            ("get", "listAttachments", Access::Session, 200),
            ("post", "uploadAttachment", Access::Mutation, 201),
        ],
    );
    add(
        &mut paths,
        "/api/v1/attachments/{id}",
        &[
            ("get", "downloadAttachment", Access::Session, 200),
            ("delete", "archiveAttachment", Access::Mutation, 204),
        ],
    );
    add(
        &mut paths,
        "/api/v1/attachments/{id}/unarchive",
        &[("post", "unarchiveAttachment", Access::Mutation, 204)],
    );
    add(
        &mut paths,
        "/api/v1/search",
        &[("get", "search", Access::Session, 200)],
    );
    add(
        &mut paths,
        "/api/v1/audit-events",
        &[("get", "listAuditEvents", Access::Session, 200)],
    );
    add(
        &mut paths,
        "/api/v1/exports",
        &[
            ("get", "listExports", Access::Session, 200),
            ("post", "createExport", Access::Mutation, 202),
        ],
    );
    add(
        &mut paths,
        "/api/v1/exports/{id}/download",
        &[("get", "downloadExport", Access::Session, 200)],
    );
    add(
        &mut paths,
        "/api/v1/admin/settings",
        &[
            ("get", "getSettings", Access::Session, 200),
            ("patch", "updateSettings", Access::Mutation, 200),
        ],
    );
    add(
        &mut paths,
        "/api/v1/admin/status",
        &[("get", "getAdminStatus", Access::Session, 200)],
    );

    for (path, method) in [
        ("/api/v1/invitations", "post"),
        ("/api/v1/pages/{id}/attachments", "post"),
        ("/api/v1/exports", "post"),
    ] {
        parameter(
            &mut paths,
            path,
            method,
            "Idempotency-Key",
            "header",
            true,
            json!({"type":"string","format":"uuid"}),
        );
    }
    for (path, method, name, required, schema) in [
        (
            "/api/v1/pages",
            "get",
            "spaceId",
            true,
            json!({"type":"string","format":"uuid"}),
        ),
        (
            "/api/v1/pages/{id}/attachments",
            "post",
            "size",
            true,
            json!({"type":"integer","format":"int64","minimum":0}),
        ),
        (
            "/api/v1/pages/{id}/attachments",
            "get",
            "archived",
            false,
            json!({"type":"boolean","default":false}),
        ),
        (
            "/api/v1/pages/{id}/attachments",
            "get",
            "after",
            false,
            json!({"type":"string","format":"uuid"}),
        ),
        (
            "/api/v1/exports",
            "get",
            "after",
            false,
            json!({"type":"string","format":"uuid"}),
        ),
        (
            "/api/v1/exports",
            "get",
            "limit",
            false,
            json!({"type":"integer","minimum":1,"maximum":100,"default":100}),
        ),
        (
            "/api/v1/audit-events",
            "get",
            "after",
            false,
            json!({"type":"string","format":"uuid"}),
        ),
        (
            "/api/v1/audit-events",
            "get",
            "limit",
            false,
            json!({"type":"integer","minimum":1,"maximum":100,"default":100}),
        ),
    ] {
        parameter(&mut paths, path, method, name, "query", required, schema);
    }
    for (name, schema) in [
        ("actor", json!({"type":"string","format":"uuid"})),
        ("type", json!({"type":"string","maxLength":128})),
        ("target", json!({"type":"string","format":"uuid"})),
        ("targetType", json!({"type":"string","maxLength":128})),
        ("from", json!({"type":"string","format":"date-time"})),
        ("to", json!({"type":"string","format":"date-time"})),
    ] {
        parameter(
            &mut paths,
            "/api/v1/audit-events",
            "get",
            name,
            "query",
            false,
            schema,
        );
    }
    for (name, required, schema) in [
        ("q", true, json!({"type":"string","maxLength":500})),
        ("space", false, json!({"type":"string","format":"uuid"})),
        ("author", false, json!({"type":"string","format":"uuid"})),
        ("from", false, json!({"type":"string","format":"date-time"})),
        ("to", false, json!({"type":"string","format":"date-time"})),
    ] {
        parameter(
            &mut paths,
            "/api/v1/search",
            "get",
            name,
            "query",
            required,
            schema,
        );
    }
    paths.get_mut("/api/v1/pages/{id}/attachments").unwrap()["post"]["requestBody"] = json!({
        "required":true,"content":{"multipart/form-data":{"schema":{"type":"object"}}}
    });
    paths.get_mut("/api/v1/attachments/{id}").unwrap()["get"]["responses"]["200"]["content"] =
        json!({"application/octet-stream":{"schema":{"type":"string","format":"binary"}}});
    paths.get_mut("/api/v1/exports/{id}/download").unwrap()["get"]["responses"]["200"]["content"] =
        json!({"application/x-tar":{"schema":{"type":"string","format":"binary"}}});

    json!({
        "openapi":"3.1.0",
        "info":{"title":"Knowledge Platform API","version":env!("CARGO_PKG_VERSION")},
        "paths":paths,
        "components":{
            "securitySchemes":{
                "sessionCookie":{"type":"apiKey","in":"cookie","name":"knowledge_session","description":"HttpOnly session cookie; browser JavaScript never reads it."},
                "csrfToken":{"type":"apiKey","in":"header","name":"X-CSRF-Token"}
            },
            "schemas":{
                "Problem":{"type":"object","required":["type","title","status","detail","code"],"properties":{"type":{"type":"string"},"title":{"type":"string"},"status":{"type":"integer"},"detail":{"type":"string"},"code":{"type":"string"}}},
                "Session":{"type":"object","required":["sessionId","userId","workspaceId","workspaceAdmin"],"properties":{"sessionId":{"type":"string","format":"uuid"},"userId":{"type":"string","format":"uuid"},"workspaceId":{"type":"string","format":"uuid"},"workspaceAdmin":{"type":"boolean"}}},
                "Csrf":{"type":"object","required":["token"],"properties":{"token":{"type":"string"}}},
                "Settings":{"type":"object","required":["attachmentRecoveryDays","exportRetentionHours"],"properties":{"attachmentRecoveryDays":{"type":"integer","minimum":1,"maximum":3650},"exportRetentionHours":{"type":"integer","minimum":1,"maximum":720}}},
                "Subject":{"type":"object","required":["id","type","label","status"],"properties":{"id":{"type":"string","format":"uuid"},"type":{"enum":["user","group"]},"label":{"type":"string"},"status":{"enum":["active"]}}},
                "PageRevision":{"type":"object","required":["id","pageId","version","title","markdown","properties","authorId","createdAt"],"properties":{"id":{"type":"string","format":"uuid"},"pageId":{"type":"string","format":"uuid"},"version":{"type":"integer"},"title":{"type":"string"},"markdown":{"type":"string"},"properties":{"type":"object"},"authorId":{"type":"string","format":"uuid"},"createdAt":{"type":"string","format":"date-time"}}}
            }
        }
    })
}

/// Stable compact JSON used by both HTTP and the generator binary.
pub fn json() -> String {
    serde_json::to_string(&document()).expect("static OpenAPI document serializes")
}
