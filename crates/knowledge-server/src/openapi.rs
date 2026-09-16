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
    let success_response = if success == 204 {
        response("Success")
    } else {
        json_response("Success", success_schema(id))
    };
    let mut responses = Map::new();
    responses.insert(success.to_string(), success_response);
    for status in error_statuses(id, access) {
        let description = match status {
            401 => "Authentication required",
            403 => "Forbidden",
            404 => "Not found",
            409 => "Conflict",
            416 => "Range not satisfiable",
            422 => "Validation failed",
            429 => "Rate limited",
            500 => "Internal error",
            503 => "Service unavailable",
            _ => unreachable!("known response status"),
        };
        responses.insert(status.to_string(), problem(description));
    }
    let mut value = json!({"operationId":id,"responses":responses});
    value["security"] = match access {
        Access::Public => json!([]),
        Access::Session => json!([{"sessionCookie":[]}]),
        Access::Mutation => json!([{"sessionCookie":[],"csrfToken":[]}]),
    };
    value
}

fn error_statuses(id: &str, access: Access) -> Vec<u16> {
    if matches!(id, "getLiveness" | "getOpenApi") {
        return Vec::new();
    }
    if id == "getReadiness" {
        return vec![503];
    }
    let mut statuses = match access {
        Access::Public => vec![500, 503],
        Access::Session => vec![401, 500, 503],
        Access::Mutation => vec![401, 403, 500, 503],
    };
    if matches!(
        id,
        "listInvitations"
            | "listUsers"
            | "listArchivedGroups"
            | "listArchivedSpaces"
            | "listSubjects"
            | "listSpaceGrants"
            | "listAuditEvents"
            | "listExports"
            | "getSettings"
            | "getAdminStatus"
    ) {
        statuses.push(403);
    }
    if matches!(
        id,
        "acceptInvitation"
            | "revokeInvitation"
            | "updateUser"
            | "revokeUserSessions"
            | "getGroup"
            | "updateGroup"
            | "archiveGroup"
            | "restoreGroup"
            | "addGroupMember"
            | "removeGroupMember"
            | "getSpace"
            | "updateSpace"
            | "archiveSpace"
            | "restoreSpace"
            | "listSubjects"
            | "listSpaceGrants"
            | "createSpaceGrant"
            | "updateSpaceGrant"
            | "deleteSpaceGrant"
            | "listPages"
            | "getPage"
            | "updatePage"
            | "archivePage"
            | "movePage"
            | "unarchivePage"
            | "listPageRevisions"
            | "getPageRevision"
            | "restorePageRevision"
            | "listBacklinks"
            | "upsertPagePermission"
            | "deletePagePermission"
            | "listAttachments"
            | "uploadAttachment"
            | "downloadAttachment"
            | "archiveAttachment"
            | "unarchiveAttachment"
            | "downloadExport"
    ) {
        statuses.push(404);
    }
    if matches!(
        id,
        "login"
            | "createInvitation"
            | "acceptInvitation"
            | "updateUser"
            | "createGroup"
            | "updateGroup"
            | "createSpace"
            | "updateSpace"
            | "createSpaceGrant"
            | "updateSpaceGrant"
            | "createPage"
            | "updatePage"
            | "movePage"
            | "restorePageRevision"
            | "upsertPagePermission"
            | "uploadAttachment"
            | "createExport"
            | "updateSettings"
            | "listArchivedGroups"
            | "listArchivedSpaces"
            | "listSubjects"
            | "listAttachments"
            | "search"
            | "listAuditEvents"
            | "listExports"
    ) {
        statuses.push(422);
    }
    if matches!(
        id,
        "createInvitation"
            | "acceptInvitation"
            | "updateUser"
            | "createGroup"
            | "updateGroup"
            | "archiveGroup"
            | "restoreGroup"
            | "addGroupMember"
            | "removeGroupMember"
            | "createSpace"
            | "updateSpace"
            | "archiveSpace"
            | "restoreSpace"
            | "createSpaceGrant"
            | "updateSpaceGrant"
            | "deleteSpaceGrant"
            | "createPage"
            | "updatePage"
            | "archivePage"
            | "movePage"
            | "unarchivePage"
            | "restorePageRevision"
            | "upsertPagePermission"
            | "deletePagePermission"
            | "uploadAttachment"
            | "archiveAttachment"
            | "unarchiveAttachment"
            | "createExport"
            | "updateSettings"
    ) {
        statuses.push(409);
    }
    if matches!(
        id,
        "login" | "createInvitation" | "acceptInvitation" | "uploadAttachment" | "createExport"
    ) {
        statuses.push(429);
    }
    statuses.sort_unstable();
    statuses.dedup();
    statuses
}

fn problem(description: &str) -> Value {
    let mut value = response(description);
    value["content"] =
        json!({"application/problem+json":{"schema":{"$ref":"#/components/schemas/Problem"}}});
    value
}

fn response(description: &str) -> Value {
    json!({
        "description":description,
        "headers":{"X-Request-ID":{"required":true,"schema":{"type":"string","format":"uuid"}}}
    })
}

fn json_response(description: &str, schema: Value) -> Value {
    let mut value = response(description);
    value["content"] = json!({"application/json":{"schema":schema}});
    value
}

fn reference(name: &str) -> Value {
    json!({"$ref":format!("#/components/schemas/{name}")})
}

fn array(name: &str) -> Value {
    json!({"type":"array","items":reference(name)})
}

fn success_schema(id: &str) -> Value {
    match id {
        "getLiveness" => reference("Liveness"),
        "getReadiness" => reference("Readiness"),
        "getOpenApi" => reference("OpenApiDocument"),
        "login" | "getSession" => reference("Session"),
        "getCsrf" => reference("Csrf"),
        "listInvitations" => array("Invitation"),
        "createInvitation" => reference("InvitationCreated"),
        "acceptInvitation" => reference("UserCreated"),
        "listUsers" => array("User"),
        "updateUser" => reference("User"),
        "listGroups" => array("Group"),
        "listArchivedGroups" | "listArchivedSpaces" => reference("ArchivedList"),
        "createGroup" | "getGroup" | "updateGroup" => reference("Group"),
        "listSpaces" => array("Space"),
        "createSpace" | "getSpace" | "updateSpace" => reference("Space"),
        "listSubjects" => reference("SubjectList"),
        "listSpaceGrants" => array("SpaceGrant"),
        "createSpaceGrant" | "updateSpaceGrant" => reference("SpaceGrant"),
        "listPages" => array("PageTreeItem"),
        "createPage" | "getPage" => reference("Page"),
        "updatePage" | "restorePageRevision" => reference("PageVersion"),
        "listPageRevisions" => array("RevisionSummary"),
        "getPageRevision" => reference("PageRevision"),
        "listBacklinks" => array("PageReference"),
        "upsertPagePermission" => reference("PagePermission"),
        "listAttachments" => array("Attachment"),
        "uploadAttachment" => reference("Attachment"),
        "search" => array("SearchHit"),
        "listAuditEvents" => reference("AuditEventList"),
        "listExports" => reference("ExportList"),
        "createExport" => reference("Export"),
        "getSettings" | "updateSettings" => reference("Settings"),
        "getAdminStatus" => reference("AdminStatus"),
        _ => reference("EmptyResponse"),
    }
}

fn add(paths: &mut BTreeMap<String, Value>, path: &str, operations: &[(&str, &str, Access, u16)]) {
    let mut item = Map::new();
    for (method, id, access, success) in operations {
        let mut value = operation(id, *access, *success);
        let parameters: Vec<Value> = path
            .split('/')
            .filter_map(|segment| segment.strip_prefix('{')?.strip_suffix('}'))
            .map(|name| {
                let schema = if name == "version" {
                    json!({"type":"integer","minimum":1})
                } else if matches!(name, "token" | "subject") {
                    json!({"type":"string"})
                } else {
                    json!({"type":"string","format":"uuid"})
                };
                json!({"name":name,"in":"path","required":true,"schema":schema})
            })
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
        "/api/v1/openapi.json",
        &[("get", "getOpenApi", Access::Public, 200)],
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
        &[("post", "movePage", Access::Mutation, 204)],
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

    for (path, method, required) in [
        ("/api/v1/invitations", "post", true),
        ("/api/v1/pages/{id}/attachments", "post", false),
        ("/api/v1/exports", "post", true),
    ] {
        parameter(
            &mut paths,
            path,
            method,
            "Idempotency-Key",
            "header",
            required,
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
            "/api/v1/groups/archived",
            "get",
            "after",
            false,
            json!({"type":"string","format":"uuid"}),
        ),
        (
            "/api/v1/groups/archived",
            "get",
            "limit",
            false,
            json!({"type":"integer","minimum":1,"maximum":100,"default":100}),
        ),
        (
            "/api/v1/spaces/archived",
            "get",
            "after",
            false,
            json!({"type":"string","format":"uuid"}),
        ),
        (
            "/api/v1/spaces/archived",
            "get",
            "limit",
            false,
            json!({"type":"integer","minimum":1,"maximum":100,"default":100}),
        ),
        (
            "/api/v1/spaces/{id}/subjects",
            "get",
            "after",
            false,
            json!({"type":"string"}),
        ),
        (
            "/api/v1/spaces/{id}/subjects",
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
    parameter(
        &mut paths,
        "/api/v1/attachments/{id}",
        "get",
        "Range",
        "header",
        false,
        json!({"type":"string","pattern":"^bytes=[0-9]+-[0-9]*$"}),
    );
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
        "required":true,"content":{"multipart/form-data":{"schema":{"$ref":"#/components/schemas/AttachmentUpload"}}}
    });
    paths.get_mut("/api/v1/attachments/{id}").unwrap()["get"]["responses"]["200"]["content"] =
        json!({"application/octet-stream":{"schema":{"type":"string","format":"binary"}}});
    paths.get_mut("/api/v1/attachments/{id}").unwrap()["get"]["responses"]["206"] = {
        let mut value = json_response(
            "Partial attachment",
            json!({"type":"string","format":"binary"}),
        );
        value["content"] =
            json!({"application/octet-stream":{"schema":{"type":"string","format":"binary"}}});
        value["headers"]["Content-Range"] = json!({"required":true,"schema":{"type":"string"}});
        value["headers"]["Accept-Ranges"] =
            json!({"required":true,"schema":{"type":"string","enum":["bytes"]}});
        value
    };
    paths.get_mut("/api/v1/attachments/{id}").unwrap()["get"]["responses"]["416"] =
        problem("Range not satisfiable");
    paths.get_mut("/api/v1/exports/{id}/download").unwrap()["get"]["responses"]["200"]["content"] =
        json!({"application/x-tar":{"schema":{"type":"string","format":"binary"}}});
    paths.get_mut("/api/v1/auth/login").unwrap()["post"]["responses"]["200"]["headers"]["Set-Cookie"] =
        json!({"required":true,"schema":{"type":"string"}});
    paths.get_mut("/api/v1/auth/logout").unwrap()["post"]["responses"]["204"]["headers"]["Set-Cookie"] =
        json!({"required":true,"schema":{"type":"string"}});

    for (operation_id, schema) in [
        ("login", "LoginRequest"),
        ("createInvitation", "InvitationCreateRequest"),
        ("acceptInvitation", "InvitationAcceptRequest"),
        ("updateUser", "UserUpdateRequest"),
        ("createGroup", "GroupCreateRequest"),
        ("updateGroup", "GroupUpdateRequest"),
        ("createSpace", "SpaceCreateRequest"),
        ("updateSpace", "SpaceUpdateRequest"),
        ("createSpaceGrant", "SpaceGrantCreateRequest"),
        ("updateSpaceGrant", "SpaceGrantUpdateRequest"),
        ("createPage", "PageCreateRequest"),
        ("updatePage", "PageUpdateRequest"),
        ("movePage", "PageMoveRequest"),
        ("restorePageRevision", "RevisionRestoreRequest"),
        ("upsertPagePermission", "PagePermissionRequest"),
        ("createExport", "ExportCreateRequest"),
        ("updateSettings", "Settings"),
    ] {
        for item in paths.values_mut() {
            for operation in item.as_object_mut().expect("path item object").values_mut() {
                if operation["operationId"] == operation_id {
                    operation["requestBody"] = json!({
                        "required":true,
                        "content":{"application/json":{"schema":reference(schema)}}
                    });
                }
            }
        }
    }

    json!({
        "openapi":"3.1.0",
        "info":{"title":"Knowledge Platform API","version":env!("CARGO_PKG_VERSION")},
        "paths":paths,
        "components":{
            "securitySchemes":{
                "sessionCookie":{"type":"apiKey","in":"cookie","name":"knowledge_session","description":"HttpOnly session cookie; browser JavaScript never reads it."},
                "csrfToken":{"type":"apiKey","in":"header","name":"X-CSRF-Token"}
            },
            "schemas":schemas()
        }
    })
}

fn schemas() -> Value {
    let uuid = || json!({"type":"string","format":"uuid"});
    let date = || json!({"type":"string","format":"date-time"});
    json!({
        "Problem":{"type":"object","additionalProperties":false,"required":["type","title","status","detail","code"],"properties":{"type":{"type":"string"},"title":{"type":"string"},"status":{"type":"integer"},"detail":{"type":"string"},"code":{"type":"string"}}},
        "Liveness":{"type":"object","additionalProperties":false,"required":["status"],"properties":{"status":{"type":"string","enum":["ok"]}}},
        "Readiness":{"type":"object","additionalProperties":false,"required":["status","database","migrations","masterKey","objectStorage"],"properties":{"status":{"type":"string","enum":["ready","not_ready"]},"database":{"type":"boolean"},"migrations":{"type":"boolean"},"masterKey":{"type":"boolean"},"objectStorage":{"type":"boolean"}}},
        "OpenApiDocument":{"type":"object","additionalProperties":true,"required":["openapi","info","paths","components"],"properties":{"openapi":{"type":"string"},"info":{"type":"object"},"paths":{"type":"object"},"components":{"type":"object"}}},
        "Session":{"type":"object","additionalProperties":false,"required":["sessionId","userId","workspaceId","workspaceAdmin"],"properties":{"sessionId":uuid(),"userId":uuid(),"workspaceId":uuid(),"workspaceAdmin":{"type":"boolean"}}},
        "Csrf":{"type":"object","additionalProperties":false,"required":["token"],"properties":{"token":{"type":"string"}}},
        "LoginRequest":{"type":"object","additionalProperties":false,"required":["email","password"],"properties":{"email":{"type":"string","format":"email"},"password":{"type":"string","format":"password","minLength":12}}},
        "InvitationCreateRequest":{"type":"object","additionalProperties":false,"required":["email","workspaceRole"],"properties":{"email":{"type":"string","format":"email"},"workspaceRole":{"type":"string","enum":["admin","member"]}}},
        "InvitationAcceptRequest":{"type":"object","additionalProperties":false,"required":["displayName","password"],"properties":{"displayName":{"type":"string","minLength":1},"password":{"type":"string","format":"password","minLength":12}}},
        "InvitationCreated":{"type":"object","additionalProperties":false,"required":["id","token","expiresAt"],"properties":{"id":uuid(),"token":{"type":"string"},"expiresAt":date()}},
        "Invitation":{"type":"object","additionalProperties":false,"required":["id","email","workspaceRole","status","expiresAt"],"properties":{"id":uuid(),"email":{"type":["string","null"],"format":"email"},"workspaceRole":{"type":"string","enum":["admin","member"]},"status":{"type":"string"},"expiresAt":date()}},
        "UserCreated":{"type":"object","additionalProperties":false,"required":["id","workspaceId"],"properties":{"id":uuid(),"workspaceId":uuid()}},
        "User":{"type":"object","additionalProperties":false,"required":["id","email","displayName","workspaceRole","status"],"properties":{"id":uuid(),"email":{"type":["string","null"],"format":"email"},"displayName":{"type":"string"},"workspaceRole":{"type":"string","enum":["admin","member"]},"status":{"type":"string","enum":["active","suspended","deleted","invited"]}}},
        "UserUpdateRequest":{"type":"object","additionalProperties":false,"minProperties":1,"properties":{"status":{"type":"string","enum":["active","suspended"]},"workspaceRole":{"type":"string","enum":["admin","member"]}}},
        "Group":{"type":"object","additionalProperties":false,"required":["id","name","description","memberIds","archivedAt"],"properties":{"id":uuid(),"name":{"type":"string"},"description":{"type":["string","null"]},"memberIds":{"type":"array","items":uuid()},"archivedAt":{"oneOf":[date(),{"type":"null"}]}}},
        "GroupCreateRequest":{"type":"object","additionalProperties":false,"required":["name"],"properties":{"name":{"type":"string","minLength":1},"description":{"type":["string","null"]}}},
        "GroupUpdateRequest":{"type":"object","additionalProperties":false,"properties":{"name":{"type":"string","minLength":1},"description":{"type":["string","null"]}}},
        "ArchivedItem":{"type":"object","additionalProperties":false,"required":["id","name","archivedAt"],"properties":{"id":uuid(),"name":{"type":"string"},"archivedAt":date()}},
        "ArchivedList":{"type":"object","additionalProperties":false,"required":["items","nextCursor"],"properties":{"items":{"type":"array","items":reference("ArchivedItem")},"nextCursor":{"oneOf":[uuid(),{"type":"null"}]}}},
        "Space":{"type":"object","required":["id","name","description","effectiveRole","archivedAt"],"properties":{"id":uuid(),"name":{"type":"string"},"description":{"type":["string","null"]},"effectiveRole":{"type":"string","enum":["admin","owner","editor","viewer","none"]},"archivedAt":{"oneOf":[date(),{"type":"null"}]}}},
        "SpaceCreateRequest":{"type":"object","additionalProperties":false,"required":["name"],"properties":{"name":{"type":"string","minLength":1},"description":{"type":["string","null"]}}},
        "SpaceUpdateRequest":{"type":"object","additionalProperties":false,"properties":{"name":{"type":"string","minLength":1},"description":{"type":["string","null"]}}},
        "Subject":{"type":"object","additionalProperties":false,"required":["id","type","label","status"],"properties":{"id":uuid(),"type":{"type":"string","enum":["user","group"]},"label":{"type":"string"},"status":{"type":"string","enum":["active"]}}},
        "SubjectList":{"type":"object","additionalProperties":false,"required":["items","nextCursor"],"properties":{"items":{"type":"array","items":reference("Subject")},"nextCursor":{"type":["string","null"]}}},
        "SpaceGrant":{"type":"object","additionalProperties":false,"required":["id","subjectType","subjectId","role"],"properties":{"id":uuid(),"subjectType":{"type":"string","enum":["user","group"]},"subjectId":uuid(),"role":{"type":"string","enum":["owner","editor","viewer"]}}},
        "SpaceGrantCreateRequest":{"type":"object","additionalProperties":false,"required":["subjectType","subjectId","role"],"properties":{"subjectType":{"type":"string","enum":["user","group"]},"subjectId":uuid(),"role":{"type":"string","enum":["owner","editor","viewer"]}}},
        "SpaceGrantUpdateRequest":{"type":"object","additionalProperties":false,"required":["role"],"properties":{"role":{"type":"string","enum":["owner","editor","viewer"]}}},
        "Page":{"type":"object","additionalProperties":false,"required":["id","spaceId","parentId","stableKey","title","markdown","properties","restricted","version"],"properties":{"id":uuid(),"spaceId":uuid(),"parentId":{"oneOf":[uuid(),{"type":"null"}]},"stableKey":uuid(),"title":{"type":"string"},"markdown":{"type":"string"},"properties":{"type":"object","additionalProperties":true},"restricted":{"type":"boolean"},"version":{"type":"integer","minimum":1}}},
        "PageTreeItem":{"type":"object","additionalProperties":false,"required":["id","parentId","sortKey","version","restricted"],"properties":{"id":uuid(),"parentId":{"oneOf":[uuid(),{"type":"null"}]},"sortKey":{"type":"string"},"version":{"type":"integer","minimum":1},"restricted":{"type":"boolean"}}},
        "PageVersion":{"type":"object","additionalProperties":false,"required":["id","version"],"properties":{"id":uuid(),"version":{"type":"integer","minimum":1}}},
        "PageCreateRequest":{"type":"object","additionalProperties":false,"required":["spaceId","title","markdown","properties"],"properties":{"spaceId":uuid(),"parentId":{"oneOf":[uuid(),{"type":"null"}]},"beforeId":{"oneOf":[uuid(),{"type":"null"}]},"title":{"type":"string"},"markdown":{"type":"string"},"properties":{"type":"object","additionalProperties":true},"restricted":{"type":"boolean","default":false}}},
        "PageUpdateRequest":{"type":"object","additionalProperties":false,"required":["title","markdown","properties","restricted","expectedVersion"],"properties":{"title":{"type":"string"},"markdown":{"type":"string"},"properties":{"type":"object","additionalProperties":true},"restricted":{"type":"boolean"},"expectedVersion":{"type":"integer","minimum":1}}},
        "PageMoveRequest":{"type":"object","additionalProperties":false,"required":["spaceId"],"properties":{"spaceId":uuid(),"parentId":{"oneOf":[uuid(),{"type":"null"}]},"beforeId":{"oneOf":[uuid(),{"type":"null"}]}}},
        "RevisionSummary":{"type":"object","additionalProperties":false,"required":["version","authorId","createdAt"],"properties":{"version":{"type":"integer","minimum":1},"authorId":uuid(),"createdAt":date()}},
        "PageRevision":{"type":"object","additionalProperties":false,"required":["id","pageId","version","title","markdown","properties","authorId","createdAt"],"properties":{"id":uuid(),"pageId":uuid(),"version":{"type":"integer","minimum":1},"title":{"type":"string"},"markdown":{"type":"string"},"properties":{"type":"object","additionalProperties":true},"authorId":uuid(),"createdAt":date()}},
        "RevisionRestoreRequest":{"type":"object","additionalProperties":false,"required":["version","expectedVersion"],"properties":{"version":{"type":"integer","minimum":1},"expectedVersion":{"type":"integer","minimum":1}}},
        "PageReference":{"type":"object","additionalProperties":false,"required":["id"],"properties":{"id":uuid()}},
        "PagePermissionRequest":{"type":"object","additionalProperties":false,"required":["role"],"properties":{"role":{"type":"string","enum":["viewer","editor"]}}},
        "PagePermission":{"type":"object","additionalProperties":false,"required":["subjectType","subjectId","role"],"properties":{"subjectType":{"type":"string","enum":["user","group"]},"subjectId":uuid(),"role":{"type":"string","enum":["viewer","editor"]}}},
        "Attachment":{"type":"object","additionalProperties":false,"required":["id","pageId","name","mime","size","archived"],"properties":{"id":uuid(),"pageId":uuid(),"name":{"type":"string"},"mime":{"type":"string"},"size":{"type":"integer","format":"int64"},"archived":{"type":"boolean"}}},
        "AttachmentUpload":{"type":"object","additionalProperties":false,"required":["file"],"properties":{"file":{"type":"string","format":"binary"}}},
        "SearchHit":{"type":"object","additionalProperties":false,"required":["pageId","title","snippet","updatedAt"],"properties":{"pageId":uuid(),"title":{"type":"string"},"snippet":{"type":"string"},"updatedAt":date()}},
        "AuditEvent":{"type":"object","required":["id","eventType","targetType","createdAt"],"properties":{"id":uuid(),"eventType":{"type":"string"},"targetType":{"type":"string"},"targetId":{"oneOf":[uuid(),{"type":"null"}]},"createdAt":date(),"metadata":{"type":"object","additionalProperties":true}}},
        "AuditEventList":{"type":"object","additionalProperties":false,"required":["events","nextCursor"],"properties":{"events":{"type":"array","items":reference("AuditEvent")},"nextCursor":{"oneOf":[uuid(),{"type":"null"}]}}},
        "Export":{"type":"object","additionalProperties":false,"required":["id","spaceId","status","expiresAt","createdAt"],"properties":{"id":uuid(),"spaceId":uuid(),"status":{"type":"string","enum":["queued","running","ready","failed","expired"]},"expiresAt":{"oneOf":[date(),{"type":"null"}]},"createdAt":date()}},
        "ExportList":{"type":"object","additionalProperties":false,"required":["exports","nextCursor"],"properties":{"exports":{"type":"array","items":reference("Export")},"nextCursor":{"oneOf":[uuid(),{"type":"null"}]}}},
        "ExportCreateRequest":{"type":"object","additionalProperties":false,"required":["spaceId"],"properties":{"spaceId":uuid()}},
        "Settings":{"type":"object","additionalProperties":false,"required":["attachmentRecoveryDays","exportRetentionHours"],"properties":{"attachmentRecoveryDays":{"type":"integer","minimum":1,"maximum":3650},"exportRetentionHours":{"type":"integer","minimum":1,"maximum":720}}},
        "AdminStatus":{"type":"object","additionalProperties":false,"required":["version","components","migrationState","queuedJobs","activeExports"],"properties":{"version":{"type":"string"},"components":{"type":"object","additionalProperties":false,"required":["database","migrations","masterKey","objectStorage"],"properties":{"database":{"type":"boolean"},"migrations":{"type":"boolean"},"masterKey":{"type":"boolean"},"objectStorage":{"type":"boolean"}}},"migrationState":{"type":"string","enum":["current","pending"]},"queuedJobs":{"type":"integer"},"activeExports":{"type":"integer"}}},
        "EmptyResponse":{"type":"object","additionalProperties":false}
    })
}

/// Stable compact JSON used by both HTTP and the generator binary.
pub fn json() -> String {
    serde_json::to_string(&document()).expect("static OpenAPI document serializes")
}
