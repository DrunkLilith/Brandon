//! Conservative, lossless edits of one top-level frontmatter field.
//! Unsupported YAML remains readable and is never reserialized by the editor.
use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const MAX_YAML: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PropertyKind {
    Text,
    List,
    Number,
    Checkbox,
    Date,
    Datetime,
    Tags,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyPatch {
    pub key: String,
    pub kind: PropertyKind,
    pub value: Value,
    #[serde(default)]
    pub remove: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyInspection {
    pub values: Map<String, Value>,
    pub editable: BTreeMap<String, bool>,
    pub error: Option<String>,
}

struct Frontmatter<'a> {
    prefix: &'a str,
    yaml: &'a str,
    suffix: &'a str,
    newline: &'static str,
}

fn frontmatter(source: &str) -> Result<Option<Frontmatter<'_>>, String> {
    if source.len() > 16 * 1024 * 1024 {
        return Err("Заметка превышает лимит 16 МиБ".into());
    }
    let (bom, rest) = if let Some(rest) = source.strip_prefix('\u{feff}') {
        ("\u{feff}", rest)
    } else {
        ("", source)
    };
    let newline = if rest.starts_with("---\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    if !rest.starts_with(&format!("---{newline}")) {
        return Ok(None);
    }
    let start = bom.len() + 3 + newline.len();
    let mut offset = start;
    for line in source[start..].split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Ok(Some(Frontmatter {
                prefix: &source[..start],
                yaml: &source[start..offset],
                suffix: &source[offset..],
                newline,
            }));
        }
        offset += line.len();
        if offset - start > MAX_YAML {
            return Err("YAML превышает лимит 256 КиБ".into());
        }
    }
    Err("Не закрыт блок YAML (---). Исправьте исходник перед редактированием свойств.".into())
}

pub fn parse_yaml_mapping(source: &str) -> Result<Value, String> {
    if source.len() > MAX_YAML {
        return Err("YAML превышает лимит 256 КиБ".into());
    }
    let value: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(source).map_err(|error| format!("Некорректный YAML: {error}"))?;
    if value.is_null() {
        return Ok(Value::Object(Map::new()));
    }
    if !value.is_mapping() {
        return Err("Ожидается YAML-объект с уникальными ключами".into());
    }
    serde_json::to_value(value).map_err(|error| format!("Неподдерживаемая структура YAML: {error}"))
}

// Find a top-level block-mapping key without treating ':' in quoted keys as a delimiter.
fn key_line(line: &str) -> Option<(String, usize)> {
    if line.starts_with(char::is_whitespace) || line.starts_with('#') {
        return None;
    }
    let mut quoted = None;
    let mut escaped = false;
    for (i, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if quoted == Some('"') && ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == '"' || ch == '\'' {
            if quoted == Some(ch) {
                quoted = None;
            } else if quoted.is_none() {
                quoted = Some(ch);
            }
        }
        if ch == ':'
            && quoted.is_none()
            && line[i + 1..].chars().next().is_none_or(char::is_whitespace)
        {
            let key: serde_yaml_ng::Value = serde_yaml_ng::from_str(line[..i].trim()).ok()?;
            return key.as_str().map(|s| (s.to_owned(), i));
        }
    }
    None
}

fn spans(
    yaml: &str,
    values: &Map<String, Value>,
) -> Result<BTreeMap<String, (usize, usize)>, String> {
    // Anchors, aliases, tags and merge keys can make a local edit non-local. Be conservative.
    if yaml.lines().any(|line| {
        line.split_whitespace()
            .any(|word| word.starts_with(['&', '*', '!']))
            || line.trim_start().starts_with("<<:")
    }) {
        return Err(
            "YAML с anchors, aliases, tags или merge keys редактируется только в исходнике".into(),
        );
    }
    let mut found = Vec::new();
    let mut offset = 0;
    for line in yaml.split_inclusive('\n') {
        let plain = line.trim_end_matches(['\n', '\r']);
        if let Some((key, colon)) = key_line(plain) {
            found.push((key, offset, colon));
        } else if !plain.is_empty()
            && !plain.starts_with(char::is_whitespace)
            && !plain.starts_with('#')
        {
            return Err("Этот синтаксис YAML доступен только в исходнике".into());
        }
        offset += line.len();
    }
    if found.len() != values.len() || found.iter().any(|(key, _, _)| !values.contains_key(key)) {
        return Err(
            "Не удалось безопасно определить границы свойств; откройте исходник YAML".into(),
        );
    }
    Ok(found
        .iter()
        .enumerate()
        .map(|(i, (key, start, _))| {
            (
                key.clone(),
                (
                    *start,
                    found.get(i + 1).map_or(yaml.len(), |(_, start, _)| *start),
                ),
            )
        })
        .collect())
}

fn simple(value: &Value) -> bool {
    value.is_null()
        || value.is_string()
        || value.is_number()
        || value.is_boolean()
        || value.as_array().is_some_and(|items| {
            items
                .iter()
                .all(|v| v.is_string() || v.is_number() || v.is_boolean())
        })
}

// Keep comments verbatim, including an inline scalar comment. Refuse complex block values.
fn comments(block: &str, newline: &str) -> Result<(String, String), String> {
    let lines: Vec<_> = block.split_inclusive('\n').collect();
    let first = lines
        .first()
        .copied()
        .unwrap_or("")
        .trim_end_matches(['\r', '\n']);
    let (_, colon) = key_line(first).ok_or("Не удалось прочитать ключ YAML")?;
    let tail = first[colon + 1..].trim_start();
    if tail.starts_with(['|', '>', '{', '&', '*', '!']) {
        return Err("Сложное значение доступно только в исходнике YAML".into());
    }
    let mut quote = None;
    let mut escaped = false;
    let mut inline = String::new();
    let quoted_or_flow = tail.starts_with(['\'', '"', '[']);
    for (i, ch) in first.char_indices().filter(|(i, _)| *i > colon) {
        if escaped {
            escaped = false;
            continue;
        }
        if quote == Some('"') && ch == '\\' {
            escaped = true;
            continue;
        }
        if quoted_or_flow && (ch == '\'' || ch == '"') {
            if quote == Some(ch) {
                quote = None
            } else if quote.is_none()
                && (first[colon + 1..i].trim().is_empty()
                    || tail.starts_with('[') && first[..i].trim_end().ends_with(['[', ',']))
            {
                quote = Some(ch)
            }
        }
        if ch == '#' && quote.is_none() && first[..i].ends_with(char::is_whitespace) {
            inline = format!(" {}", &first[i..]);
            break;
        }
    }
    let mut kept = String::new();
    for line in lines.iter().skip(1) {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            kept.push_str(line);
        } else if !line.trim_start().starts_with("- ") || line.contains('#') {
            return Err(
                "Сложное значение или комментарий внутри списка: используйте исходник YAML".into(),
            );
        }
    }
    // Do not leak a different newline style into files without a trailing newline.
    if !kept.is_empty() && !kept.ends_with('\n') {
        kept.push_str(newline);
    }
    Ok((inline, kept))
}

pub fn inspect_properties(source: &str) -> PropertyInspection {
    let result = (|| {
        let Some(front) = frontmatter(source)? else {
            return Ok((Map::new(), BTreeMap::new()));
        };
        let values = parse_yaml_mapping(front.yaml)?
            .as_object()
            .cloned()
            .unwrap_or_default();
        let ranges = spans(front.yaml, &values)?;
        let editable = values
            .iter()
            .map(|(key, value)| {
                (
                    key.clone(),
                    simple(value)
                        && ranges.get(key).is_some_and(|(a, b)| {
                            comments(&front.yaml[*a..*b], front.newline).is_ok()
                        }),
                )
            })
            .collect();
        Ok::<_, String>((values, editable))
    })();
    match result {
        Ok((values, editable)) => PropertyInspection {
            values,
            editable,
            error: None,
        },
        Err(error) => PropertyInspection {
            values: Map::new(),
            editable: BTreeMap::new(),
            error: Some(error),
        },
    }
}

fn validate(patch: &PropertyPatch) -> Result<(), String> {
    if patch.key.is_empty() || patch.key.len() > 256 || patch.key.chars().any(char::is_control) {
        return Err("Некорректное имя свойства (1–256 байт)".into());
    }
    if patch.remove {
        return Ok(());
    }
    let value = &patch.value;
    let valid = match patch.kind {
        PropertyKind::Text => value.as_str().is_some_and(|s| !s.contains(['\r', '\n'])),
        PropertyKind::Number => value.is_number(),
        PropertyKind::Checkbox => value.is_boolean(),
        PropertyKind::List => value.as_array().is_some_and(|items| {
            items.len() <= 1024
                && items
                    .iter()
                    .all(|v| v.is_string() || v.is_number() || v.is_boolean())
        }),
        PropertyKind::Tags => {
            patch.key == "tags"
                && value.as_array().is_some_and(|items| {
                    items.len() <= 1024
                        && items.iter().all(|v| {
                            v.as_str().is_some_and(|s| {
                                !s.is_empty()
                                    && !s.contains(char::is_whitespace)
                                    && !s.starts_with('#')
                            })
                        })
                })
        }
        PropertyKind::Date => value
            .as_str()
            .is_some_and(|s| s.len() == 10 && NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok()),
        PropertyKind::Datetime => value.as_str().is_some_and(|s| {
            NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").is_ok()
                || NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").is_ok()
                || chrono::DateTime::parse_from_rfc3339(s).is_ok()
        }),
    };
    if !valid {
        return Err("Значение не соответствует выбранному типу свойства".into());
    }
    if value.to_string().len() > MAX_YAML / 2 {
        return Err("Слишком длинное значение свойства".into());
    }
    Ok(())
}

pub fn patch_property(source: &str, patch: &PropertyPatch) -> Result<String, String> {
    validate(patch)?;
    // JSON is a YAML-compatible flow scalar/list encoding and quotes ambiguous strings.
    let field = format!(
        "{}: {}",
        serde_json::to_string(&patch.key).map_err(|e| e.to_string())?,
        patch.value
    );
    let Some(front) = frontmatter(source)? else {
        if patch.remove {
            return Ok(source.into());
        }
        let (bom, body) = source
            .strip_prefix('\u{feff}')
            .map_or(("", source), |s| ("\u{feff}", s));
        let newline = if body.contains("\r\n") { "\r\n" } else { "\n" };
        return Ok(format!(
            "{bom}---{newline}{field}{newline}---{newline}{body}"
        ));
    };
    let values = parse_yaml_mapping(front.yaml)?
        .as_object()
        .cloned()
        .unwrap_or_default();
    let ranges = spans(front.yaml, &values)?;
    let mut yaml = front.yaml.to_owned();
    if let Some((a, b)) = ranges.get(&patch.key) {
        if !simple(&values[&patch.key]) {
            return Err("Вложенное свойство не изменено: используйте исходник YAML".into());
        }
        let (inline, kept) = comments(&yaml[*a..*b], front.newline)?;
        let replacement = if patch.remove {
            kept.clone()
        } else {
            format!("{field}{inline}{}{kept}", front.newline)
        };
        // A removed scalar's comment remains as a standalone comment.
        let replacement = if patch.remove && !inline.is_empty() {
            format!("{}{}{kept}", inline.trim_start(), front.newline)
        } else {
            replacement
        };
        yaml.replace_range(*a..*b, &replacement);
    } else if !patch.remove {
        yaml.push_str(&format!("{field}{}", front.newline));
    }
    parse_yaml_mapping(&yaml)?;
    Ok(format!("{}{yaml}{}", front.prefix, front.suffix))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn patch(key: &str, value: Value, kind: PropertyKind) -> PropertyPatch {
        PropertyPatch {
            key: key.into(),
            value,
            kind,
            remove: false,
        }
    }
    #[test]
    fn single_field_preserves_crlf_bom_comments_nested_and_body() {
        let input = "\u{feff}---\r\n# before\r\nstatus: todo # state\r\n# keep\r\nnested:\r\n  list: [a, {b: 2}]\r\n---\r\n# Text\r\n";
        let output = patch_property(
            input,
            &patch("status", Value::from("done"), PropertyKind::Text),
        )
        .unwrap();
        assert_eq!(
            output,
            input.replace("status: todo", "\"status\": \"done\"")
        );
        assert!(!inspect_properties(input).editable["nested"]);
    }
    #[test]
    fn add_delete_and_empty_frontmatter_keep_text() {
        let mut p = patch(
            "tags",
            serde_json::json!(["test", "другое"]),
            PropertyKind::Tags,
        );
        let output = patch_property("# Note\n", &p).unwrap();
        assert!(output.ends_with("---\n# Note\n"));
        p.remove = true;
        assert_eq!(patch_property(&output, &p).unwrap(), "---\n---\n# Note\n");
    }
    #[test]
    fn block_list_comments_and_quoted_colon_key_survive() {
        let input = "---\n\"a: b\":\n  - one\n  # retained\n  - two\nother: yes\n---\nbody";
        let out = patch_property(
            input,
            &patch("a: b", serde_json::json!(["new"]), PropertyKind::List),
        )
        .unwrap();
        assert_eq!(
            out,
            "---\n\"a: b\": [\"new\"]\n  # retained\nother: yes\n---\nbody"
        );
    }
    #[test]
    fn unsupported_and_duplicate_yaml_fail_closed() {
        for input in [
            "---\n{a: 1}\n---\n",
            "---\na: 1\na: 2\n---\n",
            "---\na: &x ok\nb: *x\n---\n",
            "---\na: 1\n",
            "---\na: |\n  words\n---\n",
        ] {
            assert!(
                patch_property(input, &patch("a", Value::from("new"), PropertyKind::Text)).is_err(),
                "{input}"
            );
        }
    }
    #[test]
    fn validates_types_without_coercion() {
        for (kind, value) in [
            (PropertyKind::Number, serde_json::json!("1+2")),
            (PropertyKind::Checkbox, serde_json::json!("false")),
            (PropertyKind::Date, serde_json::json!("2026-02-30")),
            (PropertyKind::Tags, serde_json::json!(["two words"])),
        ] {
            assert!(patch_property("", &patch("tags", value, kind)).is_err());
        }
        assert!(
            patch_property(
                "",
                &patch(
                    "date",
                    Value::from("2026-10-19T12:30"),
                    PropertyKind::Datetime
                )
            )
            .is_ok()
        );
    }

    #[test]
    fn apostrophes_in_plain_scalars_do_not_erase_comments() {
        let input = "---\na: can't # retain this\nb: \"# value\" # keep this too\n---\n";
        let output =
            patch_property(input, &patch("a", Value::from("new"), PropertyKind::Text)).unwrap();
        assert!(output.contains("\"a\": \"new\" # retain this\n"));
        let mut remove = patch("b", Value::Null, PropertyKind::Text);
        remove.remove = true;
        let output = patch_property(&output, &remove).unwrap();
        assert!(output.contains("# keep this too\n"));
        let flow = "---\na: [can't] # preserve flow comment\n---\n";
        let output = patch_property(
            flow,
            &patch("a", serde_json::json!(["new"]), PropertyKind::List),
        )
        .unwrap();
        assert!(output.contains("# preserve flow comment\n"));
    }
}
