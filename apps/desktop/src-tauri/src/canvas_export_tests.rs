use super::*;
use crate::canvas_render_scene::Node;
fn fixture() -> (tempfile::TempDir, Vault, PrepareRequest) {
    let directory = tempfile::tempdir().unwrap();
    let vault = Vault::open(directory.path()).unwrap();
    let source = json!({"nodes":[
        {"id":"frame","type":"group","x":0,"y":0,"width":400,"height":240,"label":"Scene","brandon":{"version":1,"frame":{"version":1,"preset":"custom","notes":"PRIVATE SPEAKER NOTES"}}},
        {"id":"card","type":"text","x":30,"y":70,"width":200,"height":100,"text":"Исходный текст"},
        {"id":"outside","type":"text","x":2000,"y":0,"width":100,"height":100,"text":"OUTSIDE SECRET"}],"edges":[],"foreign":{"keep":73}}).to_string();
    fs::write(directory.path().join("Board.canvas"), &source).unwrap();
    let nodes = [json!({"id":"frame","kind":"group","x":0,"y":0,"width":400,"height":240,"text":"Scene","fill":"#211e2733","border":"#51465f"}),
        json!({"id":"card","kind":"text","x":30,"y":70,"width":200,"height":100,"text":"Исходный текст","fill":"#292531","border":"#51465f"})].into_iter().map(|v|serde_json::from_value::<Node>(v).unwrap()).collect();
    let request = PrepareRequest {
        root: vault.root().to_string_lossy().into(),
        request_id: "test-export-1".into(),
        source_path: "Board.canvas".into(),
        source,
        frame_id: "frame".into(),
        scene: ExportScene {
            version: 1,
            bounds: Rect {
                x: 0.,
                y: 0.,
                width: 400.,
                height: 240.,
            },
            scale: 1,
            transparent: false,
            nodes,
            edges: vec![],
        },
    };
    (directory, vault, request)
}
fn flag() -> AtomicBool {
    AtomicBool::new(false)
}
#[test]
fn real_prepare_publish_has_two_files_without_source_notes_unrelated_data_or_overwrite() {
    let (directory, vault, request) = fixture();
    let source = request.source.clone();
    let job = prepare(vault, request, &flag()).unwrap();
    let destination = tempfile::tempdir().unwrap();
    let saved = publish(&job, destination.path(), &flag(), |from, to| {
        renamore::rename_exclusive(from, to).map_err(|e| e.to_string())
    })
    .unwrap();
    let png = fs::read(&saved.png_path).unwrap();
    let report = fs::read_to_string(&saved.report_path).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(digest(&png), job.report["pngSha256"]);
    let pixels = tiny_skia::Pixmap::decode_png(&png).unwrap();
    assert_eq!((pixels.width(), pixels.height()), (400, 240));
    assert_eq!(fs::read_dir(&saved.directory).unwrap().count(), 2);
    assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 1);
    assert!(!report.contains("PRIVATE SPEAKER"));
    assert!(!report.contains("OUTSIDE SECRET"));
    assert!(!report.contains("Исходный текст"));
    assert_eq!(job.report["render"]["nodes"], 2);
    assert_eq!(job.report["format"], "brandon.canvas-png-export");
    assert_eq!(
        fs::read_to_string(directory.path().join("Board.canvas")).unwrap(),
        source
    );
    assert!(!directory.path().join(".brandon").exists());
}
#[test]
fn dirty_or_forged_source_geometry_text_membership_and_future_versions_are_refused() {
    let (_directory, vault, request) = fixture();
    let mut changed = request.clone();
    changed.source.push(' ');
    assert!(prepare(vault.clone(), changed, &flag()).is_err());
    let mut changed = request.clone();
    changed.scene.nodes[1].text = "Forged".into();
    assert!(source_scene(&changed).is_err());
    let mut changed = request.clone();
    changed.scene.nodes.reverse();
    assert!(source_scene(&changed).is_err());
    let mut changed = request.clone();
    changed.scene.nodes.pop();
    assert!(source_scene(&changed).is_err());
    let mut changed = request.clone();
    changed.scene.bounds.width += 1.;
    assert!(source_scene(&changed).is_err());
    let mut changed = request.clone();
    changed.source = changed.source.replace("\"version\":1", "\"version\":999");
    assert!(source_scene(&changed).is_err());
    let mut changed = request.clone();
    changed.source_path = "../Board.canvas".into();
    assert!(prepare(vault, changed, &flag()).is_err());
    assert!(validate_id("../bad").is_err());
    assert!(validate_id("").is_err());
    assert!(validate_id("uuid-7-okay").is_ok());
}
#[test]
fn changes_cancel_and_failed_commit_leave_no_partial_publication() {
    let (directory, vault, request) = fixture();
    let job = prepare(vault, request, &flag()).unwrap();
    let output = tempfile::tempdir().unwrap();
    let stopped = AtomicBool::new(true);
    assert!(
        publish(&job, output.path(), &stopped, |_, _| panic!(
            "cancelled before commit"
        ))
        .is_err()
    );
    assert!(
        publish(&job, output.path(), &flag(), |_, _| Err(
            "injected commit failure".into()
        ))
        .is_err()
    );
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
    assert!(
        publish(&job, directory.path(), &flag(), |_, _| panic!(
            "cannot write into vault"
        ))
        .is_err()
    );
    assert!(destination(&job.vault, Path::new("relative")).is_err());
    fs::write(directory.path().join("Board.canvas"), "external change").unwrap();
    assert!(revalidate(&job, &flag()).is_err());
    assert!(
        publish(&job, output.path(), &flag(), |_, _| panic!(
            "stale source before commit"
        ))
        .is_err()
    );
    assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
}
#[test]
fn exclusive_directory_commit_refuses_files_empty_directories_and_nonempty_directories() {
    let output = tempfile::tempdir().unwrap();
    for kind in 0..3 {
        let stage = tempfile::tempdir_in(output.path()).unwrap();
        fs::write(stage.path().join("frame.png"), b"new").unwrap();
        let target = output.path().join(format!("exists-{kind}"));
        if kind == 0 {
            fs::write(&target, b"original").unwrap()
        } else {
            fs::create_dir(&target).unwrap();
            if kind == 2 {
                fs::write(target.join("keep"), b"original").unwrap()
            }
        }
        assert!(renamore::rename_exclusive(stage.path(), &target).is_err());
        assert_eq!(fs::read(stage.path().join("frame.png")).unwrap(), b"new");
        if kind == 0 {
            assert_eq!(fs::read(&target).unwrap(), b"original")
        } else {
            assert!(!target.join("frame.png").exists());
            if kind == 2 {
                assert_eq!(fs::read(target.join("keep")).unwrap(), b"original")
            }
        }
    }
}
#[test]
fn canonical_task_revision_and_missing_asset_changes_invalidate_prepared_jobs() {
    let (directory, vault, mut request) = fixture();
    let task = vault
        .create_task_record("Canonical task", None)
        .unwrap()
        .record;
    let mut raw: Value = serde_json::from_str(&request.source).unwrap();
    raw["nodes"][1] = json!({"id":"card","type":"file","x":30,"y":70,"width":200,"height":100,"file":task.note_path,"brandon":{"version":1,"taskId":task.id}});
    request.source = raw.to_string();
    fs::write(directory.path().join("Board.canvas"), &request.source).unwrap();
    request.scene.nodes[1].kind = "file".into();
    request.scene.nodes[1].text = "Untrusted cached task title".into();
    let mut job = prepare(vault.clone(), request, &flag()).unwrap();
    assert_eq!(job.task_revisions[&task.id], Some(task.revision()));
    vault
        .patch_managed_task(
            &task.id,
            &task.revision(),
            &knowledge_vault::ManagedTaskPatch {
                status: Some("done".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(revalidate(&job, &flag()).is_err());
    job.task_revisions.clear();
    job.asset_revisions.insert("missing.png".into(), None);
    assert!(revalidate(&job, &flag()).is_ok());
    fs::write(
        directory.path().join("missing.png"),
        b"changed even if corrupt",
    )
    .unwrap();
    assert!(revalidate(&job, &flag()).is_err());
}
#[test]
fn scope_switch_cancels_workers_and_ttl_frees_only_finished_jobs() {
    let (_directory, vault, request) = fixture();
    let cancel = Arc::new(flag());
    let state = CanvasExportState::default();
    *state.inner.lock().unwrap() = Some(Phase::Preparing {
        vault: vault.clone(),
        id: "pending".into(),
        cancel: cancel.clone(),
        at: Instant::now() - TTL - Duration::from_secs(1),
    });
    expire(&mut state.inner.lock().unwrap());
    assert!(cancel.load(Ordering::Relaxed));
    assert!(state.inner.lock().unwrap().is_some());
    cancel.store(false, Ordering::Relaxed);
    state.scope_changed();
    assert!(cancel.load(Ordering::Relaxed));
    assert!(state.inner.lock().unwrap().is_some());
    let mut job = prepare(vault, request, &flag()).unwrap();
    job.at = Instant::now() - TTL - Duration::from_secs(1);
    *state.inner.lock().unwrap() = Some(Phase::Ready(Arc::new(job)));
    expire(&mut state.inner.lock().unwrap());
    assert!(state.inner.lock().unwrap().is_none());
}
