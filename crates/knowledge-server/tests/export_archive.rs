use std::{collections::HashMap, io::Read};

use knowledge_crypto::{KeyPurpose, KeyVersion, WorkspaceKey, stream::AttachmentStream};
use knowledge_server::export_archive::{
    ArchiveEncryptor, ArchiveInput, ArchiveLimits, ArchivePlan, AttachmentInput, PageInput,
    SourceId, TarPiece,
};
use serde_json::{Value, json};
use uuid::Uuid;

fn ids() -> (Uuid, Uuid, Uuid, Uuid) {
    (
        Uuid::from_u128(1),
        Uuid::from_u128(2),
        Uuid::from_u128(3),
        Uuid::from_u128(4),
    )
}

fn fixture() -> (ArchivePlan, HashMap<SourceId, Vec<u8>>) {
    let (space_id, page_id, child_id, attachment_id) = ids();
    let markdown = b"# Page\n\nbody\n".to_vec();
    let child_markdown = b"child\n".to_vec();
    let attachment = b"attachment bytes".to_vec();
    let plan = ArchivePlan::new(
        ArchiveInput {
            space_id,
            space_name: "../../ Dangerous \"space\"".into(),
            pages: vec![
                PageInput {
                    page_id: child_id,
                    stable_key: Uuid::from_u128(30),
                    parent_page_id: Some(page_id),
                    revision_id: Uuid::from_u128(31),
                    revision: 7,
                    title: "Child: --- ../".into(),
                    properties: json!({"tags": ["a", "---", "../"]}),
                    markdown_size: child_markdown.len() as u64,
                },
                PageInput {
                    page_id,
                    stable_key: Uuid::from_u128(20),
                    parent_page_id: None,
                    revision_id: Uuid::from_u128(21),
                    revision: 3,
                    title: "Main\nTitle".into(),
                    properties: json!({"owner": "../../etc"}),
                    markdown_size: markdown.len() as u64,
                },
            ],
            attachments: vec![AttachmentInput {
                attachment_id,
                page_id,
                original_name: "../../secrets.txt".into(),
                media_type: "text/plain".into(),
                size: attachment.len() as u64,
            }],
        },
        ArchiveLimits::default(),
    )
    .unwrap();
    let sources = HashMap::from([
        (SourceId::PageMarkdown(page_id), markdown),
        (SourceId::PageMarkdown(child_id), child_markdown),
        (SourceId::Attachment(attachment_id), attachment),
    ]);
    (plan, sources)
}

fn render(plan: &ArchivePlan, sources: &HashMap<SourceId, Vec<u8>>) -> Vec<u8> {
    let mut archive = Vec::new();
    for piece in plan.pieces() {
        match piece {
            TarPiece::Bytes(bytes) => archive.extend_from_slice(&bytes),
            TarPiece::Source { id, size } => {
                let source = &sources[&id];
                assert_eq!(source.len() as u64, size);
                archive.extend_from_slice(source);
            }
        }
    }
    assert_eq!(archive.len() as u64, plan.archive_size());
    archive
}

#[test]
fn posix_tar_has_safe_deterministic_paths_parseable_yaml_and_manifest_mapping() {
    let (plan, sources) = fixture();
    let first = render(&plan, &sources);
    let second = render(&plan, &sources);
    assert_eq!(first, second);

    let mut parsed = tar::Archive::new(first.as_slice());
    let mut files = HashMap::new();
    for entry in parsed.entries().unwrap() {
        let mut entry = entry.unwrap();
        let path = entry.path().unwrap().into_owned();
        assert!(!path.is_absolute());
        assert!(
            !path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
        );
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        files.insert(path.to_string_lossy().replace('\\', "/"), bytes);
    }
    let root = format!("space-{}/", Uuid::from_u128(1));
    assert_eq!(files.len(), 4);
    assert!(files.contains_key(&format!("{root}pages/{}.md", Uuid::from_u128(2))));
    assert!(files.contains_key(&format!("{root}pages/{}.md", Uuid::from_u128(3))));
    assert_eq!(
        files[&format!("{root}attachments/{}", Uuid::from_u128(4))],
        b"attachment bytes"
    );

    let page =
        std::str::from_utf8(&files[&format!("{root}pages/{}.md", Uuid::from_u128(2))]).unwrap();
    let (_, yaml, body) = page
        .split_once("---\n")
        .and_then(|(_, rest)| {
            let (yaml, body) = rest.split_once("\n---\n")?;
            Some(((), yaml, body))
        })
        .unwrap();
    let frontmatter: serde_yaml_ng::Value = serde_yaml_ng::from_str(yaml).unwrap();
    assert_eq!(
        frontmatter["pageId"].as_str(),
        Some("00000000-0000-0000-0000-000000000002")
    );
    assert_eq!(
        frontmatter["stableKey"].as_str(),
        Some("00000000-0000-0000-0000-000000000014")
    );
    assert_eq!(frontmatter["revision"].as_u64(), Some(3));
    assert_eq!(frontmatter["title"].as_str(), Some("Main\nTitle"));
    assert_eq!(body.as_bytes(), b"# Page\n\nbody\n");

    let manifest: Value = serde_json::from_slice(&files[&format!("{root}manifest.json")]).unwrap();
    assert_eq!(manifest["space"]["name"], "../../ Dangerous \"space\"");
    assert_eq!(
        manifest["pages"][0]["pageId"],
        Uuid::from_u128(2).to_string()
    );
    assert_eq!(
        manifest["pages"][1]["parentPageId"],
        Uuid::from_u128(2).to_string()
    );
    assert_eq!(
        manifest["attachments"][0]["originalName"],
        "../../secrets.txt"
    );
    assert_eq!(
        manifest["attachments"][0]["path"],
        format!("attachments/{}", Uuid::from_u128(4))
    );
}

#[test]
fn plan_rejects_duplicates_unknown_parents_and_every_bound_without_truncation() {
    let (space_id, page_id, _, _) = ids();
    let page = PageInput {
        page_id,
        stable_key: Uuid::from_u128(20),
        parent_page_id: None,
        revision_id: Uuid::from_u128(21),
        revision: 1,
        title: "title".into(),
        properties: json!({}),
        markdown_size: 1,
    };
    let base = ArchiveInput {
        space_id,
        space_name: "space".into(),
        pages: vec![page.clone()],
        attachments: vec![],
    };
    assert!(
        ArchivePlan::new(
            ArchiveInput {
                pages: vec![page.clone(), page.clone()],
                ..base.clone()
            },
            ArchiveLimits::default()
        )
        .is_err()
    );
    assert!(
        ArchivePlan::new(
            ArchiveInput {
                pages: vec![PageInput {
                    parent_page_id: Some(Uuid::from_u128(99)),
                    ..page.clone()
                }],
                ..base.clone()
            },
            ArchiveLimits::default()
        )
        .is_err()
    );
    assert!(
        ArchivePlan::new(
            base.clone(),
            ArchiveLimits {
                max_pages: 0,
                ..ArchiveLimits::default()
            }
        )
        .is_err()
    );
    assert!(
        ArchivePlan::new(
            base.clone(),
            ArchiveLimits {
                max_manifest_bytes: 1,
                ..ArchiveLimits::default()
            }
        )
        .is_err()
    );
    assert!(
        ArchivePlan::new(
            base,
            ArchiveLimits {
                max_archive_bytes: 1024,
                ..ArchiveLimits::default()
            }
        )
        .is_err()
    );
    assert!(
        ArchivePlan::new(
            ArchiveInput {
                space_id,
                space_name: "space".into(),
                pages: vec![PageInput {
                    properties: json!({"large": "x".repeat(70_000)}),
                    ..page
                }],
                attachments: vec![],
            },
            ArchiveLimits::default()
        )
        .is_err(),
        "page metadata must be bounded before the archive plan retains it"
    );
}

#[test]
fn empty_accessible_space_still_exports_a_manifest_only_archive() {
    let space_id = Uuid::from_u128(41);
    let plan = ArchivePlan::new(
        ArchiveInput {
            space_id,
            space_name: "Empty".into(),
            pages: vec![],
            attachments: vec![],
        },
        ArchiveLimits::default(),
    )
    .unwrap();
    let bytes = render(&plan, &HashMap::new());
    let mut parsed = tar::Archive::new(bytes.as_slice());
    let entries = parsed
        .entries()
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].path().unwrap().to_string_lossy(),
        format!("space-{space_id}/manifest.json")
    );
}

#[test]
fn archive_encryptor_is_bounded_domain_separated_and_checks_declared_size() {
    let plaintext: Vec<u8> = (0..150_000).map(|n| (n % 251) as u8).collect();
    let workspace = Uuid::from_u128(7);
    let export = Uuid::from_u128(8);
    let workspace_key = WorkspaceKey::generate().unwrap();
    let purpose = workspace_key.derive(KeyPurpose::Attachment).unwrap();
    let mut encryptor = ArchiveEncryptor::new(purpose, workspace, export, plaintext.len() as u64);
    let mut encrypted = Vec::new();
    encrypted.extend_from_slice(encryptor.header());
    for part in plaintext.chunks(777) {
        for frame in encryptor.push(part).unwrap() {
            assert!(frame.len() <= 64 * 1024 + 28);
            encrypted.extend_from_slice(&frame);
        }
    }
    encrypted.extend_from_slice(&encryptor.finish().unwrap());
    assert_eq!(encrypted.len() as u64, encryptor.ciphertext_size());

    let stream = AttachmentStream::new_export(
        workspace_key.derive(KeyPurpose::Attachment).unwrap(),
        KeyVersion(1),
        workspace.as_bytes(),
        export.as_bytes(),
        plaintext.len() as u64,
    );
    let mut decrypted = Vec::new();
    for index in 0..stream.chunk_count() {
        let range = stream.frame_range(index).unwrap();
        decrypted.extend(
            stream
                .decrypt_chunk(index, &encrypted[range.start as usize..range.end as usize])
                .unwrap(),
        );
    }
    let footer = stream.footer_range();
    stream
        .verify_footer(&encrypted[footer.start as usize..footer.end as usize])
        .unwrap();
    assert_eq!(decrypted, plaintext);
    let wrong = AttachmentStream::new_export(
        workspace_key.derive(KeyPurpose::Attachment).unwrap(),
        KeyVersion(1),
        workspace.as_bytes(),
        Uuid::from_u128(9).as_bytes(),
        plaintext.len() as u64,
    );
    let range = wrong.frame_range(0).unwrap();
    assert!(
        wrong
            .decrypt_chunk(0, &encrypted[range.start as usize..range.end as usize])
            .is_err()
    );

    let purpose = workspace_key.derive(KeyPurpose::Attachment).unwrap();
    let mut short = ArchiveEncryptor::new(purpose, workspace, export, 10);
    assert!(short.push(b"short").is_ok());
    assert!(short.finish().is_err());
    let purpose = workspace_key.derive(KeyPurpose::Attachment).unwrap();
    let mut long = ArchiveEncryptor::new(purpose, workspace, export, 3);
    assert!(long.push(b"four").is_err());

    let purpose = workspace_key.derive(KeyPurpose::Attachment).unwrap();
    let mut bounded = ArchiveEncryptor::new(purpose, workspace, export, 70_000);
    assert!(
        bounded.push(&vec![0; 65_537]).is_err(),
        "a caller cannot force an oversized pending buffer"
    );
}
