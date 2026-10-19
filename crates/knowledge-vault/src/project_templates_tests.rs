use super::*;
fn input(choice: &TemplateChoice, name: &str) -> TemplateInput {
    TemplateInput {
        id: choice.id.clone(),
        fingerprint: choice.fingerprint.clone(),
        name: name.into(),
        parent: String::new(),
        date: "2026-10-19".into(),
        request_id: uuid::Uuid::now_v7().to_string(),
    }
}
fn fixture() -> (tempfile::TempDir, Vault) {
    let d = tempfile::tempdir().unwrap();
    let v = Vault::open(d.path()).unwrap();
    (d, v)
}
fn own(v: &Vault) -> String {
    let c = &v.project_template_catalog().choices[0];
    v.project_template_source(&c.id, &c.fingerprint, true)
        .unwrap()
}
#[test]
fn all_eight_own_starters_create_real_portable_projects_without_task_row_copies() {
    let (directory, vault) = fixture();
    let catalog = vault.project_template_catalog();
    assert!(catalog.error.is_none());
    assert_eq!(catalog.choices.len(), 8);
    assert!(!directory.path().join(".brandon").exists());
    for (i, choice) in catalog.choices.iter().enumerate() {
        assert!(choice.error.is_none(), "{:?}", choice.error);
        let input = input(choice, &format!("Проект {i}"));
        let preview = vault.preview_project_template(&input).unwrap();
        assert_eq!(preview.files.len(), 3);
        assert!(!directory.path().join(&preview.path).exists());
        let result = vault
            .apply_project_template(&input, &preview.plan_hash)
            .unwrap();
        assert!(result.created);
        assert!(directory.path().join(&result.entrypoint).is_file());
        let manifest: Value = serde_json::from_slice(
            &fs::read(directory.path().join(&result.path).join(MANIFEST)).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["projectKey"], result.project_key);
        assert_eq!(manifest["files"].as_array().unwrap().len(), 3);
        for f in &preview.files {
            assert_eq!(
                fs::read_to_string(directory.path().join(&preview.path).join(&f.path)).unwrap(),
                f.content
            );
            assert!(!f.content.contains("\"taskId\""));
            assert!(!f.content.contains("{{project}}"))
        }
        let owner_file = directory
            .path()
            .join(&preview.path)
            .join(&preview.files[0].path);
        fs::write(&owner_file, "Owner's later edit").unwrap();
        let retry = vault
            .apply_project_template(&input, &preview.plan_hash)
            .unwrap();
        assert!(!retry.created);
        assert_eq!(
            fs::read_to_string(owner_file).unwrap(),
            "Owner's later edit"
        );
    }
    assert!(!directory.path().join(".brandon/tasks.sqlite3").exists());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 8);
}
#[test]
fn catalogue_cas_copy_edit_and_unknown_data_are_preserved_without_executing_them() {
    let (directory, vault) = fixture();
    let source = own(&vault);
    let source = source.replacen('{', "{\"foreignPrecision\":18446744073709551615,", 1);
    let c = inspect_project_template(&source);
    assert!(c.error.is_none());
    let catalogue = vault.project_template_catalog();
    let imported = vault
        .import_project_template(&source, catalogue.fingerprint.as_deref().unwrap(), None)
        .unwrap();
    assert_eq!(imported.choices.len(), 9);
    let before = fs::read(directory.path().join(".brandon").join(REGISTRY)).unwrap();
    assert!(
        vault
            .import_project_template(&source, "stale", None)
            .is_err()
    );
    assert_eq!(
        fs::read(directory.path().join(".brandon").join(REGISTRY)).unwrap(),
        before
    );
    let t: Template = serde_json::from_str(&source).unwrap();
    let edit = TemplateEdit {
        name: "Свой проект".into(),
        category: t.category,
        description: t.description,
        entrypoint: t.entrypoint,
        files: t
            .files
            .into_iter()
            .map(|f| TemplateFileEdit {
                path: f.path,
                content: f.content,
            })
            .collect(),
    };
    let edited = edit_project_template(&source, edit).unwrap();
    assert!(edited.contains("18446744073709551615"));
    assert!(
        vault
            .import_project_template(&edited, imported.fingerprint.as_deref().unwrap(), None)
            .is_err()
    );
    let updated = vault
        .import_project_template(
            &edited,
            imported.fingerprint.as_deref().unwrap(),
            Some(&c.fingerprint),
        )
        .unwrap();
    assert_eq!(updated.choices.len(), 9);
    assert!(updated.choices.iter().any(|e| e.name == "Свой проект"));
    assert!(
        vault
            .project_template_source(&c.id, &c.fingerprint, false)
            .is_err()
    );
    let latest = updated.choices.iter().find(|e| e.id == c.id).unwrap();
    assert_eq!(
        vault
            .project_template_source(&latest.id, &latest.fingerprint, false)
            .unwrap(),
        edited
    );
}
#[test]
fn existing_targets_scope_reopen_stale_plan_and_folder_changes_refuse_without_overwrite() {
    let (directory, vault) = fixture();
    let c = &vault.project_template_catalog().choices[0];
    let mut request = input(c, "Project");
    let preview = vault.preview_project_template(&request).unwrap();
    assert!(vault.apply_project_template(&request, "stale").is_err());
    assert!(!directory.path().join("Project").exists());
    let reopened = Vault::open(directory.path()).unwrap();
    assert!(
        reopened
            .apply_project_template(&request, &preview.plan_hash)
            .is_err()
    );
    fs::create_dir(directory.path().join("Project")).unwrap();
    assert!(
        vault
            .apply_project_template(&request, &preview.plan_hash)
            .is_err()
    );
    assert_eq!(
        fs::read_dir(directory.path().join("Project"))
            .unwrap()
            .count(),
        0
    );
    request.name = "Existing file".into();
    fs::write(directory.path().join(&request.name), "original").unwrap();
    assert!(vault.preview_project_template(&request).is_err());
    assert_eq!(
        fs::read_to_string(directory.path().join(&request.name)).unwrap(),
        "original"
    );
    request.name = "Other".into();
    request.parent = ".brandon".into();
    assert!(vault.preview_project_template(&request).is_err());
    request.parent = "../escape".into();
    assert!(vault.preview_project_template(&request).is_err());
}
#[test]
fn unknown_catalogue_version_keeps_builtins_available_and_never_replaces_registry() {
    let (directory, vault) = fixture();
    let source = own(&vault);
    fs::create_dir(directory.path().join(".brandon")).unwrap();
    let future=b"{\"format\":\"brandon.project-template-catalog\",\"version\":99,\"entries\":[],\"foreign\":73}";
    fs::write(directory.path().join(".brandon").join(REGISTRY), future).unwrap();
    let catalog = vault.project_template_catalog();
    assert!(catalog.error.is_some());
    assert!(catalog.fingerprint.is_none());
    assert_eq!(catalog.choices.len(), 8);
    assert!(
        catalog
            .choices
            .iter()
            .all(|c| c.builtin && c.error.is_none())
    );
    assert!(vault.import_project_template(&source, "", None).is_err());
    assert_eq!(
        fs::read(directory.path().join(".brandon").join(REGISTRY)).unwrap(),
        future
    );
    let request = input(&catalog.choices[0], "Still usable");
    let p = vault.preview_project_template(&request).unwrap();
    assert!(
        vault
            .apply_project_template(&request, &p.plan_hash)
            .unwrap()
            .created
    );
}
#[test]
fn paths_devices_task_ids_unknown_versions_and_macros_fail_before_any_project_write() {
    let (_directory, vault) = fixture();
    let source = own(&vault);
    let value: Value = serde_json::from_str(&source).unwrap();
    for path in [
        "../escape.md",
        ".private/secret.md",
        "CON.notes.md",
        "AUX.md",
        "node_modules/a.md",
        "a/../../x.md",
        "a:b.md",
        "a\\b.md",
    ] {
        let mut bad = value.clone();
        bad["files"][0]["path"] = json!(path);
        assert!(
            inspect_project_template(&bad.to_string()).error.is_some(),
            "{path}"
        )
    }
    for change in 0..6 {
        let mut bad = value.clone();
        match change {
            0 => bad["version"] = json!(99),
            1 => bad["entrypoint"] = json!("missing.md"),
            2 => bad["files"][1]["path"] = bad["files"][0]["path"].clone(),
            3 => bad["files"][0]["content"] = json!("{{shell:run}}"),
            4 => bad["files"][0]["content"] = json!("![[Secret.md]]"),
            _ => {
                let mut canvas: Value =
                    serde_json::from_str(bad["files"][2]["content"].as_str().unwrap()).unwrap();
                canvas["nodes"][1]["brandon"]["taskId"] = json!("existing-task");
                bad["files"][2]["content"] = json!(canvas.to_string())
            }
        };
        assert!(
            inspect_project_template(&bad.to_string()).error.is_some(),
            "change {change}"
        )
    }
}
#[test]
fn literal_expansion_is_single_pass_and_json_numbers_are_never_silently_reserialized() {
    let (_directory, vault) = fixture();
    let source = own(&vault);
    let mut raw: Value = serde_json::from_str(&source).unwrap();
    let board = raw["files"][2]["content"].as_str().unwrap().replacen(
        '{',
        "{\"foreignNumber\":1.2300e2,\"foreignZero\":0.0,",
        1,
    );
    raw["files"][2]["content"] = json!(board);
    let source = raw.to_string();
    assert!(inspect_project_template(&source).error.is_none());
    let c = vault
        .import_project_template(
            &source,
            vault
                .project_template_catalog()
                .fingerprint
                .as_deref()
                .unwrap(),
            None,
        )
        .unwrap()
        .choices
        .pop()
        .unwrap();
    let request = input(&c, "Literal {{date}}");
    let p = vault.preview_project_template(&request).unwrap();
    let canvas = &p
        .files
        .iter()
        .find(|f| f.path.ends_with(".canvas"))
        .unwrap()
        .content;
    assert!(canvas.contains("1.2300e2"));
    assert!(canvas.contains("0.0"));
    assert!(canvas.contains("Literal {{date}}"));
    for number in ["9007199254740993", "1.234567890123456789"] {
        let mut bad = raw.clone();
        bad["files"][2]["content"] =
            json!(board_replace_number(&bad["files"][2]["content"], number));
        assert!(inspect_project_template(&bad.to_string()).error.is_some())
    }
}
fn board_replace_number(value: &Value, number: &str) -> String {
    value.as_str().unwrap().replace("1.2300e2", number)
}
#[test]
fn template_removal_keeps_an_exact_recoverable_archive_and_existing_projects() {
    let (directory, vault) = fixture();
    let source = own(&vault);
    let c = vault
        .import_project_template(
            &source,
            vault
                .project_template_catalog()
                .fingerprint
                .as_deref()
                .unwrap(),
            None,
        )
        .unwrap();
    let item = c.choices.last().unwrap();
    let request = input(item, "Keep project");
    let p = vault.preview_project_template(&request).unwrap();
    vault
        .apply_project_template(&request, &p.plan_hash)
        .unwrap();
    let removed = vault
        .remove_project_template(
            &item.id,
            &item.fingerprint,
            c.fingerprint.as_deref().unwrap(),
        )
        .unwrap();
    assert_eq!(removed.catalog.choices.len(), 8);
    assert_eq!(fs::read_to_string(&removed.archive_path).unwrap(), source);
    assert!(
        directory
            .path()
            .join("Keep project/Workspace.canvas")
            .is_file()
    );
    let restored = vault
        .import_project_template(
            &fs::read_to_string(removed.archive_path).unwrap(),
            removed.catalog.fingerprint.as_deref().unwrap(),
            None,
        )
        .unwrap();
    assert_eq!(restored.choices.len(), 9);
}
#[test]
fn exports_are_fresh_external_json_files_and_never_touch_an_existing_file() {
    let (directory, vault) = fixture();
    let choice = &vault.project_template_catalog().choices[0];
    let out = tempfile::tempdir().unwrap();
    fs::write(out.path().join("keep.json"), "original").unwrap();
    let first = vault
        .export_project_template(
            &choice.id,
            &choice.fingerprint,
            out.path().to_str().unwrap(),
        )
        .unwrap();
    let second = vault
        .export_project_template(
            &choice.id,
            &choice.fingerprint,
            out.path().to_str().unwrap(),
        )
        .unwrap();
    assert_ne!(first, second);
    assert_eq!(
        fs::read_to_string(out.path().join("keep.json")).unwrap(),
        "original"
    );
    assert_eq!(
        inspect_project_template(&read_project_template(&first).unwrap()).fingerprint,
        choice.fingerprint
    );
    assert!(
        vault
            .export_project_template(
                &choice.id,
                &choice.fingerprint,
                directory.path().to_str().unwrap()
            )
            .is_err()
    );
}
#[test]
fn simultaneous_retry_publishes_one_complete_bundle_only() {
    let (directory, vault) = fixture();
    let choice = &vault.project_template_catalog().choices[0];
    let request = input(choice, "Once");
    let p = vault.preview_project_template(&request).unwrap();
    let results = std::thread::scope(|scope| {
        let left = scope.spawn(|| vault.apply_project_template(&request, &p.plan_hash));
        let right = scope.spawn(|| vault.apply_project_template(&request, &p.plan_hash));
        vec![
            left.join().unwrap().unwrap(),
            right.join().unwrap().unwrap(),
        ]
    });
    assert_eq!(results.iter().filter(|r| r.created).count(), 1);
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    assert_eq!(
        fs::read_dir(directory.path().join("Once")).unwrap().count(),
        4
    );
}
