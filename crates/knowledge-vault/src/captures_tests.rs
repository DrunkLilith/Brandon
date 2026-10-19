use super::*;
use crate::{Vault, inspect_capture_import};
use serde_json::json;
fn choice(id: &str, target: &str) -> Value {
    json!({"id":id,"name":format!("Capture {id}"),"type":"Capture","command":true,"captureTo":target,"captureToActiveFile":false,"prepend":true,"task":false,"format":{"enabled":true,"format":"- {{DATE:HH:mm}} {{VALUE}}\n"},"createFileIfItDoesntExist":{"enabled":true,"createWithTemplate":false,"template":""},"insertAfter":{"enabled":false,"after":"retained inert text"},"insertBefore":{"enabled":false},"newLineCapture":{"enabled":false},"appendLink":false,"openFile":false,"templater":{"afterCapture":"none"},"futureInactive":{"enabled":false,"keep":{"x":1}}})
}
fn source(choice: Value) -> String {
    json!({"version":"2.32.1","choices":[choice],"ai":{"providerKey":"DO-NOT-IMPORT-CONTAINER"}})
        .to_string()
}
fn setup(raw: Value) -> (tempfile::TempDir, Vault) {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    vault.import_capture(&source(raw), "one", "").unwrap();
    (dir, vault)
}
fn input() -> CaptureInput {
    CaptureInput {
        choice_id: "one".into(),
        values: BTreeMap::from([(String::new(), "Hello".into())]),
        date_time: "2026-10-19T12:34:56".into(),
    }
}
#[test]
fn inspect_refuses_effects_and_keeps_supported_neighbors() {
    let mut script = choice("script", "Inbox.md");
    script["type"] = json!("Macro");
    let mut bad = choice("bad", "Inbox.md");
    bad["eachLine"] = json!(true);
    let inspected = inspect_capture_import(
        &json!({"choices":[choice("one","Inbox.md"),script,bad]}).to_string(),
    )
    .unwrap();
    assert!(inspected[0].error.is_none());
    assert!(inspected[1].error.is_some());
    assert!(inspected[2].error.is_some());
}
#[test]
fn concurrent_private_directory_bootstrap_is_idempotent_and_not_a_false_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("contended");
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(24));
    let threads: Vec<_> = (0..24)
        .map(|_| {
            let path = path.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                task_storage::checked_directory(&path)
            })
        })
        .collect();
    for thread in threads {
        thread.join().unwrap().unwrap();
    }
    assert!(path.is_dir());
}
#[test]
fn optional_effects_cannot_be_disguised_as_disabled_unknown_shapes() {
    for (field, value) in [
        ("propertyCapture", json!({"enabled":false,"action":"set"})),
        ("dateOrigin", json!({"enabled":false,"type":"ask"})),
        ("templater", json!({"afterCapture":true})),
    ] {
        let mut raw = choice("one", "Inbox.md");
        raw[field] = value;
        assert!(
            inspect_capture_import(&source(raw)).unwrap()[0]
                .error
                .is_some(),
            "{field}"
        );
    }
}
#[cfg(windows)]
#[test]
fn existing_case_alias_resolves_to_the_actual_note_spelling() {
    let (dir, vault) = setup(choice("one", "INBOX.MD"));
    fs::write(dir.path().join("Inbox.md"), "Existing").unwrap();
    let plan = vault.preview_capture(&input()).unwrap();
    assert_eq!(plan.path, "Inbox.md");
    assert!(!plan.created);
}
#[test]
fn selected_raw_json_and_unknown_extensions_survive_but_container_secrets_do_not() {
    let dir = tempfile::tempdir().unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    let raw = serde_json::to_string_pretty(&choice("one", "Inbox.md")).unwrap();
    let original = format!(
        "{{\"version\":\"2.32.1\",\"choices\":[{raw}],\"apiKey\":\"DO-NOT-IMPORT-CONTAINER\"}}"
    );
    let imported = vault.import_capture(&original, "one", "").unwrap();
    let path = dir.path().join(".brandon/captures.v1.json");
    let stored = fs::read_to_string(&path).unwrap();
    assert!(stored.contains(&raw));
    assert!(!stored.contains("DO-NOT-IMPORT-CONTAINER"));
    assert!(stored.contains("futureInactive"));
    let mut value: Value = serde_json::from_str(&stored).unwrap();
    value["extension"] = json!({"opaque":[1,2]});
    value["choices"][0]["extension"] = json!({"keep":true});
    fs::write(&path, value.to_string()).unwrap();
    assert!(
        vault
            .import_capture(
                &source(choice("two", "Other.md")),
                "two",
                &imported.fingerprint
            )
            .is_err()
    );
    let current = vault.load_captures().unwrap();
    vault
        .import_capture(
            &source(choice("two", "Other.md")),
            "two",
            &current.fingerprint,
        )
        .unwrap();
    let final_value: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(final_value["extension"], value["extension"]);
    assert_eq!(
        final_value["choices"][0]["extension"],
        value["choices"][0]["extension"]
    );
}
#[test]
fn preview_never_creates_folders_and_value_is_never_reexpanded() {
    let mut raw = choice("one", "Journal/{{DATE:YYYY-MM-DD}}.md");
    raw["format"] = json!({"enabled":true,"format":"{{VALUE:Project}}: {{VALUE}}"});
    let (dir, vault) = setup(raw);
    let mut input = input();
    input.values.insert("Project".into(), "School".into());
    input
        .values
        .insert("".into(), "{{DATE}} <% script %>".into());
    let plan = vault.preview_capture(&input).unwrap();
    assert_eq!(plan.path, "Journal/2026-10-19.md");
    assert_eq!(plan.after, "School: {{DATE}} <% script %>");
    assert!(!dir.path().join("Journal").exists());
}
#[test]
fn apply_creates_one_note_and_immediate_replay_cannot_duplicate_it() {
    let (dir, vault) = setup(choice("one", "Journal/Inbox"));
    let plan = vault.preview_capture(&input()).unwrap();
    assert!(plan.created);
    let applied = vault.apply_capture(&input(), &plan.plan_hash).unwrap();
    assert_eq!(applied.path, "Journal/Inbox.md");
    assert_eq!(
        fs::read_to_string(dir.path().join("Journal/Inbox.md")).unwrap(),
        "- 12:34 Hello\n"
    );
    assert!(vault.apply_capture(&input(), &plan.plan_hash).is_err());
    assert_eq!(fs::read_dir(dir.path().join("Journal")).unwrap().count(), 1);
}
#[test]
fn append_and_body_start_preserve_bom_crlf_frontmatter_and_surrounding_bytes() {
    let mut raw = choice("one", "Inbox.md");
    raw["task"] = json!(true);
    raw["format"] = json!({"enabled":true,"format":"{{VALUE}}"});
    let (dir, vault) = setup(raw.clone());
    let before = "\u{feff}---\r\n# comment\r\nunknown: {keep: true}\r\n---\r\n# Original";
    fs::write(dir.path().join("Inbox.md"), before).unwrap();
    let plan = vault.preview_capture(&input()).unwrap();
    assert_eq!(plan.after, format!("{before}\r\n- [ ] Hello\r\n"));
    vault.apply_capture(&input(), &plan.plan_hash).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("Inbox.md")).unwrap(),
        plan.after
    );
    raw["prepend"] = json!(false);
    let (_dir, top) = setup(raw);
    fs::write(top.root().join("Inbox.md"), before).unwrap();
    let plan = top.preview_capture(&input()).unwrap();
    assert_eq!(
        plan.after,
        before.replace("# Original", "- [ ] Hello\r\n# Original")
    );
}
#[test]
fn body_start_capture_after_a_yaml_only_note_without_final_newline_keeps_the_closing_delimiter() {
    let mut raw = choice("one", "Inbox.md");
    raw["prepend"] = json!(false);
    raw["format"] = json!({"enabled":true,"format":"{{VALUE}}"});
    let (dir, vault) = setup(raw);
    let before = "\u{feff}---\r\nkeep: true\r\n---";
    fs::write(dir.path().join("Inbox.md"), before).unwrap();
    let plan = vault.preview_capture(&input()).unwrap();
    assert_eq!(plan.after, format!("{before}\r\nHello"));
    vault.apply_capture(&input(), &plan.plan_hash).unwrap();
    assert_eq!(
        fs::read_to_string(dir.path().join("Inbox.md")).unwrap(),
        plan.after
    );
}
#[test]
fn disk_and_registry_conflicts_never_overwrite_newer_bytes() {
    let (dir, vault) = setup(choice("one", "Inbox.md"));
    fs::write(dir.path().join("Inbox.md"), "Original").unwrap();
    let plan = vault.preview_capture(&input()).unwrap();
    fs::write(dir.path().join("Inbox.md"), "EXTERNAL").unwrap();
    assert!(vault.apply_capture(&input(), &plan.plan_hash).is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("Inbox.md")).unwrap(),
        "EXTERNAL"
    );
    let plan = vault.preview_capture(&input()).unwrap();
    let library = vault.load_captures().unwrap();
    vault
        .import_capture(
            &source(choice("two", "Other.md")),
            "two",
            &library.fingerprint,
        )
        .unwrap();
    assert!(vault.apply_capture(&input(), &plan.plan_hash).is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("Inbox.md")).unwrap(),
        "EXTERNAL"
    );
}
#[test]
fn new_target_collision_and_unclosed_frontmatter_are_explicit_errors() {
    let (dir, vault) = setup(choice("one", "Inbox.md"));
    let plan = vault.preview_capture(&input()).unwrap();
    fs::write(dir.path().join("Inbox.md"), "KEEP").unwrap();
    assert!(vault.apply_capture(&input(), &plan.plan_hash).is_err());
    assert_eq!(
        fs::read_to_string(dir.path().join("Inbox.md")).unwrap(),
        "KEEP"
    );
    let mut raw = choice("one", "Inbox.md");
    raw["prepend"] = json!(false);
    let (_dir, vault) = setup(raw);
    fs::write(vault.root().join("Inbox.md"), "---\nnot closed").unwrap();
    assert!(vault.preview_capture(&input()).is_err());
}
#[test]
fn unsupported_active_options_and_formats_never_import() {
    for (field, value) in [
        ("futureEffect", json!(true)),
        ("appendLink", json!(true)),
        ("propertyCapture", json!({"action":"set"})),
        ("dateOrigin", json!({"mode":"ask"})),
        ("captureToActiveFile", json!(true)),
        ("useSelectionAsCaptureValue", json!(true)),
        ("templater", json!({"afterCapture":"wholeFile"})),
        ("format", json!({"enabled":true,"format":"{{MACRO:bad}}"})),
        ("format", json!({"enabled":true,"format":"{{DATE:ddd}}"})),
    ] {
        let mut raw = choice("one", "Inbox.md");
        raw[field] = value;
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::open(dir.path()).unwrap();
        assert!(
            vault.import_capture(&source(raw), "one", "").is_err(),
            "{field}"
        );
        assert!(!dir.path().join(".brandon/captures.v1.json").exists());
    }
}
#[test]
fn future_registry_is_preserved_and_cannot_be_laundered_by_import() {
    let (dir, vault) = setup(choice("one", "Inbox.md"));
    let path = dir.path().join(".brandon/captures.v1.json");
    let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    value["version"] = json!(999);
    let bytes = value.to_string();
    fs::write(&path, &bytes).unwrap();
    assert!(vault.load_captures().is_err());
    assert!(
        vault
            .import_capture(&source(choice("two", "Other.md")), "two", "")
            .is_err()
    );
    assert_eq!(fs::read_to_string(path).unwrap(), bytes);
}
#[test]
fn unsafe_paths_and_path_value_injection_are_rejected() {
    for target in [
        "../Outside.md",
        "/absolute.md",
        "C:/outside.md",
        ".brandon/secret.md",
        "A/../Escape.md",
        "A\\Escape.md",
        "CON.md",
        "Daily:note.md",
        "A./x.md",
        "Other.canvas",
    ] {
        assert!(
            inspect_capture_import(&source(choice("one", target))).unwrap()[0]
                .error
                .is_some(),
            "{target}"
        );
    }
    let (dir, vault) = setup(choice("one", "{{VALUE}}.md"));
    let mut input = input();
    input.values.insert("".into(), "../escape".into());
    assert!(vault.preview_capture(&input).is_err());
    assert!(!dir.path().join("escape.md").exists());
}
#[test]
fn bounded_import_input_and_source_limits_fail_before_writes() {
    assert!(inspect_capture_import(&" ".repeat(MAX_JSON + 1)).is_err());
    let (dir, vault) = setup(choice("one", "Inbox.md"));
    let mut input = input();
    input.values.insert("".into(), "a".repeat(65537));
    assert!(vault.preview_capture(&input).is_err());
    fs::write(dir.path().join("Inbox.md"), vec![b'x'; MAX_NOTE + 1]).unwrap();
    assert!(
        vault
            .preview_capture(&super::captures_tests::input())
            .is_err()
    );
}
#[test]
fn concurrent_apply_of_one_plan_publishes_only_one_capture() {
    let (dir, vault) = setup(choice("one", "Inbox.md"));
    let plan = vault.preview_capture(&input()).unwrap();
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let vault = vault.clone();
            let hash = plan.plan_hash.clone();
            std::thread::spawn(move || vault.apply_capture(&input(), &hash).is_ok())
        })
        .collect();
    assert_eq!(
        threads
            .into_iter()
            .map(|thread| thread.join().unwrap())
            .filter(|applied| *applied)
            .count(),
        1
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("Inbox.md")).unwrap(),
        "- 12:34 Hello\n"
    );
}
#[test]
fn capture_registry_survives_task_backup_and_clean_folder_restore() {
    let (_dir, vault) = setup(choice("one", "Inbox.md"));
    let output = tempfile::tempdir().unwrap();
    let destination = tempfile::tempdir().unwrap();
    let backup = vault.backup_tasks(output.path()).unwrap();
    let restored = vault
        .restore_task_backup(backup.path, destination.path())
        .unwrap();
    let restored = Vault::open(restored.path).unwrap();
    assert_eq!(restored.load_captures().unwrap().choices[0].id, "one");
    let plan = restored.preview_capture(&input()).unwrap();
    restored.apply_capture(&input(), &plan.plan_hash).unwrap();
    assert_eq!(
        fs::read_to_string(restored.root().join("Inbox.md")).unwrap(),
        "- 12:34 Hello\n"
    );
}
#[cfg(windows)]
#[test]
fn target_and_registry_junctions_do_not_escape_the_vault() {
    let (dir, vault) = setup(choice("one", "Junction/Inbox.md"));
    let outside = tempfile::tempdir().unwrap();
    let link = dir.path().join("Junction");
    let status = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(outside.path())
        .output()
        .unwrap();
    assert!(status.status.success());
    assert!(vault.preview_capture(&input()).is_err());
    assert!(!outside.path().join("Inbox.md").exists());
}
