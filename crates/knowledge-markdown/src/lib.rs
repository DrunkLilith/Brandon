//! Portable Markdown metadata extraction for local knowledge vaults.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use regex::{Captures, Regex};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::BTreeSet, path::Path, sync::OnceLock};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentMetadata {
    pub path: String,
    pub title: String,
    pub aliases: Vec<String>,
    pub headings: Vec<Heading>,
    pub blocks: Vec<BlockReference>,
    pub tags: Vec<String>,
    pub links: Vec<DocumentLink>,
    pub tasks: Vec<TaskMetadata>,
    pub properties: Map<String, Value>,
    pub word_count: usize,
    pub modified_unix_ms: u128,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskMetadata {
    pub id: String,
    #[serde(default)]
    pub managed: bool,
    pub block_id: Option<String>,
    pub path: String,
    pub line: usize,
    pub title: String,
    pub completed: bool,
    pub scheduled: Option<String>,
    pub due: Option<String>,
    pub duration_minutes: Option<u32>,
    pub priority: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub slug: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BlockReference {
    pub id: String,
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLink {
    pub target: String,
    pub display: Option<String>,
    pub heading: Option<String>,
    pub block: Option<String>,
    pub line: usize,
    pub kind: LinkKind,
    pub resolved_path: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkKind {
    Wiki,
    Markdown,
    Embed,
}

pub fn parse_document(path: &str, source: &str, modified_unix_ms: u128) -> DocumentMetadata {
    let (frontmatter, markdown) = split_frontmatter(source);
    let properties = parse_properties(frontmatter);
    let mut aliases = property_strings(&properties, &["aliases", "alias"]);
    aliases.sort_by_key(|value| value.to_lowercase());
    aliases.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

    let mut headings = Vec::new();
    let mut markdown_links = Vec::new();
    let mut current_heading: Option<(HeadingLevel, String)> = None;
    let parser = Parser::new_ext(markdown, Options::all());
    for event in parser {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                current_heading = Some((level, String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text)) = current_heading.take() {
                    let text = text.trim().to_owned();
                    if !text.is_empty() {
                        headings.push(Heading {
                            level: heading_level(level),
                            slug: slugify(&text),
                            text,
                        });
                    }
                }
            }
            Event::Text(text) | Event::Code(text) => {
                if let Some((_, heading)) = current_heading.as_mut() {
                    heading.push_str(&text);
                }
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                let destination = dest_url.to_string();
                if !destination.starts_with("http://")
                    && !destination.starts_with("https://")
                    && !destination.starts_with("mailto:")
                    && !destination.starts_with('#')
                {
                    markdown_links.push(DocumentLink {
                        target: destination,
                        display: None,
                        heading: None,
                        block: None,
                        line: 0,
                        kind: LinkKind::Markdown,
                        resolved_path: None,
                    });
                }
            }
            _ => {}
        }
    }

    let mut links = parse_wiki_links(markdown);
    links.extend(markdown_links);
    let mut tag_set: BTreeSet<String> = property_strings(&properties, &["tags", "tag"])
        .into_iter()
        .map(|tag| tag.trim_start_matches('#').to_owned())
        .filter(|tag| !tag.is_empty())
        .collect();
    for captures in tag_regex().captures_iter(markdown) {
        if let Some(tag) = captures.get(1) {
            tag_set.insert(tag.as_str().to_owned());
        }
    }

    let title = property_strings(&properties, &["title"])
        .into_iter()
        .next()
        .or_else(|| headings.first().map(|heading| heading.text.clone()))
        .unwrap_or_else(|| {
            Path::new(path)
                .file_stem()
                .map(|value| value.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_owned())
        });

    let blocks = block_regex()
        .captures_iter(markdown)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            let id = captures.get(1)?.as_str().to_owned();
            Some(BlockReference {
                id,
                line: line_number(markdown, whole.start()),
            })
        })
        .collect();
    let tasks = parse_tasks(path, source);

    DocumentMetadata {
        path: path.to_owned(),
        title,
        aliases,
        headings,
        blocks,
        tags: tag_set.into_iter().collect(),
        links,
        tasks,
        properties,
        word_count: markdown.split_whitespace().count(),
        modified_unix_ms,
    }
}

pub fn rewrite_task_source(
    source: &str,
    line_number: usize,
    block_id: Option<&str>,
    completed: Option<bool>,
    scheduled: Option<Option<&str>>,
    due: Option<Option<&str>>,
    new_block_id: &str,
) -> Option<String> {
    let mut offset = 0;
    for (index, segment) in source.split_inclusive('\n').enumerate() {
        let ending = if segment.ends_with("\r\n") {
            "\r\n"
        } else if segment.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        let line = segment.strip_suffix(ending).unwrap_or(segment);
        let current_block = task_block_regex()
            .captures(line)
            .and_then(|captures| captures.get(1))
            .map(|value| value.as_str());
        let is_target = block_id
            .map(|expected| current_block.is_some_and(|value| value == expected))
            .unwrap_or(index + 1 == line_number);
        if is_target && task_line_regex().is_match(line) {
            let mut rewritten = line.to_owned();
            if let Some(completed) = completed {
                let checked = task_line_regex()
                    .captures(&rewritten)
                    .and_then(|captures| captures.name("checked"))?;
                rewritten.replace_range(checked.range(), if completed { "x" } else { " " });
            }

            let stable_id = current_block.unwrap_or(new_block_id).to_owned();
            rewritten = task_block_regex().replace(&rewritten, "").into_owned();
            if scheduled.is_some() || due.is_some() {
                let stored_schedule = task_scheduled_regex()
                    .captures(&rewritten)
                    .and_then(|captures| captures.get(1))
                    .map(|value| value.as_str().to_owned());
                let stored_due = task_due_regex()
                    .captures(&rewritten)
                    .and_then(|captures| captures.get(1))
                    .map(|value| value.as_str().to_owned());
                let next_schedule = scheduled
                    .map(|value| value.map(str::to_owned))
                    .unwrap_or(stored_schedule);
                let next_due = due
                    .map(|value| value.map(str::to_owned))
                    .unwrap_or(stored_due);
                rewritten = task_scheduled_regex()
                    .replace_all(&rewritten, "")
                    .into_owned();
                rewritten = task_due_regex()
                    .replace_all(&rewritten, "")
                    .trim_end()
                    .to_owned();
                if let Some(value) = next_schedule {
                    rewritten.push_str(" ⏳ ");
                    rewritten.push_str(&value);
                }
                if let Some(value) = next_due {
                    rewritten.push_str(" 📅 ");
                    rewritten.push_str(&value);
                }
            }
            rewritten = rewritten.trim_end().to_owned();
            rewritten.push_str(" ^");
            rewritten.push_str(&stable_id);

            let mut result = source.to_owned();
            result.replace_range(offset..offset + line.len(), &rewritten);
            return Some(result);
        }
        offset += segment.len();
    }
    None
}

fn parse_tasks(path: &str, source: &str) -> Vec<TaskMetadata> {
    task_line_regex()
        .captures_iter(source)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            let body = captures.name("body")?.as_str();
            let line = line_number(source, whole.start());
            let block_id = task_block_regex()
                .captures(body)
                .and_then(|captures| captures.get(1))
                .map(|value| value.as_str().to_owned());
            let scheduled = task_scheduled_regex()
                .captures(body)
                .and_then(|captures| captures.get(1))
                .map(|value| value.as_str().replace('T', " "));
            let due = task_due_regex()
                .captures(body)
                .and_then(|captures| captures.get(1))
                .map(|value| value.as_str().replace('T', " "));
            let duration_minutes = task_duration_regex().captures(body).and_then(|captures| {
                let value = captures.get(1)?.as_str().parse::<f64>().ok()?;
                let unit = captures.get(2)?.as_str();
                Some(if unit.eq_ignore_ascii_case("h") {
                    (value * 60.0).round() as u32
                } else {
                    value.round() as u32
                })
            });
            let priority = if body.contains('⏫') {
                Some("urgent".to_owned())
            } else if body.contains('🔼') {
                Some("high".to_owned())
            } else if body.contains('🔽') {
                Some("low".to_owned())
            } else if body.contains('⏬') {
                Some("lowest".to_owned())
            } else {
                None
            };
            let mut title = task_block_regex().replace(body, "").into_owned();
            title = task_scheduled_regex().replace_all(&title, "").into_owned();
            title = task_due_regex().replace_all(&title, "").into_owned();
            title = task_duration_regex().replace_all(&title, "").into_owned();
            title = task_priority_regex().replace_all(&title, "").into_owned();
            let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
            let tags = tag_regex()
                .captures_iter(body)
                .filter_map(|captures| captures.get(1).map(|value| value.as_str().to_owned()))
                .collect();
            Some(TaskMetadata {
                id: block_id
                    .clone()
                    .unwrap_or_else(|| format!("{path}::L{line}")),
                managed: false,
                block_id,
                path: path.to_owned(),
                line,
                title,
                completed: captures.name("checked")?.as_str() != " ",
                scheduled,
                due,
                duration_minutes,
                priority,
                tags,
            })
        })
        .collect()
}

pub fn rewrite_wiki_link_targets(source: &str, old_targets: &[String], new_target: &str) -> String {
    wiki_link_regex()
        .replace_all(source, |captures: &Captures<'_>| {
            let Some(body_match) = captures.name("body") else {
                return captures[0].to_owned();
            };
            let body = body_match.as_str();
            let (destination, display) = body
                .split_once('|')
                .map_or((body, None), |(left, right)| (left, Some(right)));
            let target_end = destination.find(['#', '^']).unwrap_or(destination.len());
            let target = &destination[..target_end];
            if !old_targets
                .iter()
                .any(|old| target.eq_ignore_ascii_case(old))
            {
                return captures[0].to_owned();
            }
            let suffix = &destination[target_end..];
            let embed = captures.name("embed").map_or("", |value| value.as_str());
            match display {
                Some(display) => format!("{embed}[[{new_target}{suffix}|{display}]]"),
                None => format!("{embed}[[{new_target}{suffix}]]"),
            }
        })
        .into_owned()
}

fn split_frontmatter(source: &str) -> (Option<&str>, &str) {
    let source = source.strip_prefix('\u{feff}').unwrap_or(source);
    if !source.starts_with("---\n") && !source.starts_with("---\r\n") {
        return (None, source);
    }
    let delimiter_len = if source.starts_with("---\r\n") { 5 } else { 4 };
    let remainder = &source[delimiter_len..];
    for delimiter in ["\n---\n", "\r\n---\r\n"] {
        if let Some(end) = remainder.find(delimiter) {
            let markdown_start = end + delimiter.len();
            return (Some(&remainder[..end]), &remainder[markdown_start..]);
        }
    }
    (None, source)
}

fn parse_properties(frontmatter: Option<&str>) -> Map<String, Value> {
    let Some(frontmatter) = frontmatter else {
        return Map::new();
    };
    serde_yaml_ng::from_str::<serde_yaml_ng::Value>(frontmatter)
        .ok()
        .and_then(|yaml| serde_json::to_value(yaml).ok())
        .and_then(|json| json.as_object().cloned())
        .unwrap_or_default()
}

fn property_strings(properties: &Map<String, Value>, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .find_map(|name| {
            properties
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(name))
                .map(|(_, value)| value)
        })
        .map(value_strings)
        .unwrap_or_default()
}

fn value_strings(value: &Value) -> Vec<String> {
    match value {
        Value::String(value) => value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect(),
        Value::Array(values) => values.iter().flat_map(value_strings).collect(),
        Value::Number(value) => vec![value.to_string()],
        Value::Bool(value) => vec![value.to_string()],
        _ => Vec::new(),
    }
}

fn parse_wiki_links(markdown: &str) -> Vec<DocumentLink> {
    wiki_link_regex()
        .captures_iter(markdown)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            let body = captures.name("body")?.as_str();
            let (destination, display) = body
                .split_once('|')
                .map_or((body, None), |(left, right)| (left, Some(right)));
            let mut target = destination;
            let mut heading = None;
            let mut block = None;
            if let Some((left, right)) = destination.split_once('#') {
                target = left;
                heading = Some(right.to_owned());
            } else if let Some((left, right)) = destination.split_once('^') {
                target = left;
                block = Some(right.to_owned());
            }
            let target = target.trim();
            if target.is_empty() {
                return None;
            }
            Some(DocumentLink {
                target: target.to_owned(),
                display: display.map(str::to_owned),
                heading,
                block,
                line: line_number(markdown, whole.start()),
                kind: if captures.name("embed").is_some() {
                    LinkKind::Embed
                } else {
                    LinkKind::Wiki
                },
                resolved_path: None,
            })
        })
        .collect()
}

fn line_number(source: &str, byte_offset: usize) -> usize {
    source[..byte_offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn slugify(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

fn wiki_link_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?P<embed>!)?\[\[(?P<body>[^\]]+)\]\]").expect("valid regex"))
}

fn tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?m)(?:^|[\s(])#([\p{L}\p{N}_/-]+)").expect("valid regex"))
}

fn block_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?m)\s\^([A-Za-z0-9-]+)\s*$").expect("valid regex"))
}

fn task_line_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?m)^[\t ]*[-*+][\t ]+\[(?P<checked>[ xX])\][\t ]+(?P<body>[^\r\n]+)$")
            .expect("valid regex")
    })
}

fn task_block_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"[\t ]+\^([A-Za-z0-9-]+)[\t ]*$").expect("valid regex"))
}

fn task_scheduled_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"[\t ]+⏳[\t ]*(\d{4}-\d{2}-\d{2}(?:[ T]\d{2}:\d{2})?)").expect("valid regex")
    })
}

fn task_due_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"[\t ]+📅[\t ]*(\d{4}-\d{2}-\d{2}(?:[ T]\d{2}:\d{2})?)").expect("valid regex")
    })
}

fn task_duration_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"[\t ]+⏱️?[\t ]*(\d+(?:\.\d+)?)[\t ]*([mMhH])").expect("valid regex")
    })
}

fn task_priority_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"[\t ]+(?:⏫|🔼|🔽|⏬)").expect("valid regex"))
}

#[cfg(test)]
mod tests {
    use super::{LinkKind, parse_document, rewrite_task_source, rewrite_wiki_link_targets};

    #[test]
    fn extracts_obsidian_metadata() {
        let source = "---\ntitle: Product map\naliases: [Roadmap, Plan]\ntags: [product/core]\nstatus: active\n---\n# Overview\n\nSee [[Research/Model#Results|the result]] and ![[Diagram]].\n\n#inline/tag task ^block-1\n";
        let document = parse_document("Product/Map.md", source, 42);
        assert_eq!(document.title, "Product map");
        assert_eq!(document.aliases, ["Plan", "Roadmap"]);
        assert_eq!(document.headings[0].text, "Overview");
        assert!(document.tags.contains(&"product/core".to_owned()));
        assert!(document.tags.contains(&"inline/tag".to_owned()));
        assert_eq!(document.blocks[0].id, "block-1");
        assert_eq!(document.links[0].target, "Research/Model");
        assert_eq!(document.links[0].heading.as_deref(), Some("Results"));
        assert_eq!(document.links[1].kind, LinkKind::Embed);
        assert_eq!(document.modified_unix_ms, 42);
    }

    #[test]
    fn rewrites_matching_wiki_targets_without_losing_suffixes() {
        let result = rewrite_wiki_link_targets(
            "[[Old]] [[Folder/Old#Heading|Alias]] ![[Other]]",
            &["Old".into(), "Folder/Old".into()],
            "Archive/New",
        );
        assert_eq!(
            result,
            "[[Archive/New]] [[Archive/New#Heading|Alias]] ![[Other]]"
        );
    }

    #[test]
    fn extracts_and_rewrites_portable_tasks() {
        let source = "# Plan\n\n- [ ] Ship demo ⏳ 2026-10-20 09:30 📅 2026-10-21 ⏱ 1.5h 🔼 #brandon ^task-demo\n- [x] Done\n";
        let document = parse_document("Plan.md", source, 0);

        assert_eq!(document.tasks.len(), 2);
        assert_eq!(document.tasks[0].id, "task-demo");
        assert_eq!(
            document.tasks[0].scheduled.as_deref(),
            Some("2026-10-20 09:30")
        );
        assert_eq!(document.tasks[0].due.as_deref(), Some("2026-10-21"));
        assert_eq!(document.tasks[0].duration_minutes, Some(90));
        assert_eq!(document.tasks[0].priority.as_deref(), Some("high"));
        assert_eq!(document.tasks[1].id, "Plan.md::L4");

        let rewritten = rewrite_task_source(
            source,
            4,
            None,
            Some(false),
            Some(Some("2026-10-22")),
            Some(Some("2026-10-23")),
            "task-new",
        )
        .expect("rewritten task");
        assert!(rewritten.contains("- [ ] Done ⏳ 2026-10-22 📅 2026-10-23 ^task-new"));
    }
}
