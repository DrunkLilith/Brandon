//! Literal-only template expansion. Untouched JSON tokens retain their bytes.
use super::project_templates::{Template, TemplateFile, TemplateInput, TemplateOutput, invalid};
use super::*;
use serde_json::Value;
use std::collections::BTreeSet;
pub(super) const MAX_FILE: usize = 2 * 1024 * 1024;
pub(super) fn public_path(path: &str) -> Result<()> {
    task_storage::portable_path(path)?;
    if path.len() > 512 || path.split('/').count() > 8 {
        return Err(invalid("Путь шаблона слишком длинный или глубокий"));
    }
    for part in path.split('/') {
        if safe_entry_name(part)? != part
            || part.eq_ignore_ascii_case("node_modules")
            || part.eq_ignore_ascii_case("target")
            || part.chars().any(char::is_control)
        {
            return Err(invalid("Непереносимый путь в шаблоне"));
        }
        let stem = part
            .split('.')
            .next()
            .unwrap_or("")
            .trim_end()
            .to_uppercase();
        if matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$" | "CONIN$" | "CONOUT$"
        ) || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.chars().count() == 4
            && stem.chars().last().is_some_and(char::is_numeric))
        {
            return Err(invalid("Зарезервированное имя устройства Windows"));
        }
    }
    Ok(())
}
fn substitute(value: &str, input: &TemplateInput, root: &str) -> Result<String> {
    let mut out = String::new();
    let mut rest = value;
    while let Some(at) = rest.find("{{") {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let end = tail
            .find("}}")
            .ok_or_else(|| invalid("Не закрыта переменная шаблона"))?;
        out.push_str(match &tail[..end+2]{"{{project}}"=>&input.name,"{{root}}"=>root,"{{date}}"=>&input.date,_=>return Err(invalid("Поддерживаются только {{project}}, {{root}}, {{date}}; выражения не выполняются"))});
        rest = &tail[end + 2..];
        if out.len() > MAX_FILE {
            return Err(invalid("Результат файла превышает 2 МиБ"));
        }
    }
    out.push_str(rest);
    if out.len() > MAX_FILE {
        return Err(invalid("Результат файла превышает 2 МиБ"));
    }
    Ok(out)
}
fn decimal(raw: &str) -> Option<(bool, String, i64)> {
    let lower = raw.to_ascii_lowercase();
    let (mantissa, exponent) = lower.split_once('e').unwrap_or((&lower, "0"));
    let exponent: i64 = exponent.parse().ok()?;
    let negative = mantissa.starts_with('-');
    let m = mantissa.trim_start_matches('-');
    let (whole, fraction) = m.split_once('.').unwrap_or((m, ""));
    let digits = format!("{whole}{fraction}");
    let significant = digits.trim_start_matches('0');
    let compact = significant.trim_end_matches('0');
    if compact.is_empty() {
        return Some((false, String::new(), 0));
    }
    Some((
        negative && !compact.is_empty(),
        compact.into(),
        exponent
            .checked_sub(fraction.len() as i64)?
            .checked_add((significant.len() - compact.len()) as i64)?,
    ))
}
fn json_source(source: &str, input: &TemplateInput, root: &str) -> Result<String> {
    let _: Value = serde_json::from_str(source)
        .map_err(|_| invalid("Canvas/.base шаблона должны быть JSON, не скриптом"))?;
    let bytes = source.as_bytes();
    let mut i = 0;
    let mut last = 0;
    let mut out = String::new();
    while i < bytes.len() {
        if bytes[i] == b'"' {
            let start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i += 2
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    i += 1
                }
            }
            let value: String = serde_json::from_str(&source[start..i])
                .map_err(|_| invalid("Некорректная строка JSON"))?;
            if value.contains("{{") {
                if source[i..].trim_start().starts_with(':') {
                    return Err(invalid("Переменные в ключах JSON не поддерживаются"));
                }
                let expanded = substitute(&value, input, root)?;
                out.push_str(&source[last..start]);
                out.push_str(&serde_json::to_string(&expanded).expect("JSON string"));
                last = i;
                if out.len() > MAX_FILE {
                    return Err(invalid("Результат файла превышает 2 МиБ"));
                }
            }
        } else if bytes[i] == b'-' || bytes[i].is_ascii_digit() {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || b".eE+-".contains(&bytes[i])) {
                i += 1
            }
            let token = &source[start..i];
            let value: f64 = token
                .parse()
                .map_err(|_| invalid("Некорректное число JSON"))?;
            if !value.is_finite()
                || (value.fract() == 0. && value.abs() > 9007199254740991.)
                || decimal(token) != decimal(&value.to_string())
            {
                return Err(invalid(
                    "Число JSON нельзя безопасно редактировать в Canvas; источник не изменён",
                ));
            }
        } else {
            i += 1
        }
    }
    out.push_str(&source[last..]);
    if out.len() > MAX_FILE {
        return Err(invalid("Результат файла превышает 2 МиБ"));
    }
    Ok(out)
}
fn local_target(file: &str, target: &str, root: &str, files: &BTreeSet<String>) -> bool {
    let target = target.split('#').next().unwrap_or("");
    if target.is_empty() {
        return true;
    }
    let raw = if target.starts_with(&(root.to_owned() + "/")) {
        target.to_owned()
    } else {
        let parent = file.rsplit_once('/').map_or("", |(p, _)| p);
        format!("{root}/{parent}/{target}")
    };
    let mut parts = Vec::new();
    for part in raw.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return false;
                }
            }
            _ => parts.push(part),
        }
    }
    let normalized = parts.join("/");
    files.contains(&normalized) || files.contains(&(normalized + ".md"))
}
fn namespace(value: Option<&Value>) -> Result<()> {
    if value.is_some_and(|v| !v.is_object() || v.get("version").and_then(Value::as_u64) != Some(1))
    {
        return Err(invalid("Неизвестная версия расширения в шаблоне"));
    }
    Ok(())
}
fn integer(value: Option<&Value>, min: f64, max: f64) -> bool {
    value
        .and_then(Value::as_f64)
        .is_some_and(|v| v.is_finite() && v.fract() == 0. && v >= min && v <= max)
}
fn node_extension(node: &Value) -> Result<usize> {
    let Some(e) = node.get("brandon") else {
        return Ok(0);
    };
    namespace(Some(e))?;
    if e.get("locked").is_some_and(|v| !v.is_boolean()) {
        return Err(invalid("Некорректная блокировка Canvas"));
    }
    for (key, allowed) in [
        ("fontSize", &[12., 16., 24., 32.][..]),
        ("strokeWidth", &[1., 2., 4.][..]),
    ] {
        if e.get(key)
            .is_some_and(|v| v.as_f64().is_none_or(|n| !allowed.contains(&n)))
        {
            return Err(invalid("Неизвестный размер стиля Canvas"));
        }
    }
    if e.get("primitive").is_some_and(|v| {
        !matches!(
            v.as_str(),
            Some("sticky" | "text" | "rectangle" | "ellipse" | "diamond" | "triangle" | "divider")
        )
    }) || e
        .get("textAlign")
        .is_some_and(|v| !matches!(v.as_str(), Some("left" | "center" | "right")))
    {
        return Err(invalid("Неизвестный стиль Canvas"));
    }
    for key in ["fillColor", "strokeColor", "textColor"] {
        if e.get(key).is_some_and(|v| {
            !v.as_str().is_some_and(|c| {
                key == "fillColor" && c == "transparent"
                    || c.len() == 7
                        && c.starts_with('#')
                        && c[1..].bytes().all(|b| b.is_ascii_hexdigit())
            })
        }) {
            return Err(invalid("Некорректный цвет Canvas"));
        }
    }
    if e.get("primitive").is_some() && node.get("type").and_then(Value::as_str) != Some("text") {
        return Err(invalid("Фигура требует text fallback"));
    }
    if let Some(frame) = e.get("frame") {
        namespace(Some(frame))?;
        if node.get("type").and_then(Value::as_str) != Some("group")
            || e.get("primitive").is_some()
            || e.get("ink").is_some()
            || !matches!(
                frame.get("preset").and_then(Value::as_str),
                Some(
                    "custom"
                        | "a4"
                        | "letter"
                        | "16:9"
                        | "4:3"
                        | "1:1"
                        | "mobile"
                        | "tablet"
                        | "desktop"
                )
            )
            || frame.get("notes").is_some_and(|v| {
                !v.as_str()
                    .is_some_and(|s| s.encode_utf16().count() <= 16384)
            })
        {
            return Err(invalid("Некорректный фрейм Canvas"));
        }
    }
    if let Some(ink) = e.get("ink") {
        namespace(Some(ink))?;
        if node.get("type").and_then(Value::as_str) != Some("text")
            || e.get("primitive").is_some()
            || !matches!(
                ink.get("tool").and_then(Value::as_str),
                Some("pen" | "highlighter")
            )
            || !integer(ink.get("size"), 1., 64.)
            || !integer(ink.get("basis"), 1., 10000000.)
            || !ink.get("pressure").is_some_and(Value::is_boolean)
        {
            return Err(invalid("Некорректный штрих Canvas"));
        }
        let points = ink
            .get("points")
            .and_then(Value::as_array)
            .filter(|p| !p.is_empty() && p.len() <= 2048)
            .ok_or_else(|| invalid("Некорректные точки штриха"))?;
        for point in points {
            let p = point
                .as_array()
                .filter(|p| p.len() == 3)
                .ok_or_else(|| invalid("Некорректная точка штриха"))?;
            if !integer(p.first(), 0., 65535.)
                || !integer(p.get(1), 0., 65535.)
                || !integer(p.get(2), 0., 1024.)
            {
                return Err(invalid("Точка штриха выходит за бюджет"));
            }
        }
        return Ok(points.len());
    }
    Ok(0)
}
fn base(value: &Value) -> Result<()> {
    let views = value
        .get("views")
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty() && v.len() <= 16)
        .ok_or_else(|| invalid("Шаблон .base требует от 1 до 16 явных views"))?;
    if let Some(v) = value.get("brandon") {
        namespace(Some(v))?;
        if v.get("format").and_then(Value::as_str) != Some("task-view") {
            return Err(invalid("Неизвестный рабочий формат .base"));
        }
        if let Some(filter) = v.get("filter") {
            let f = filter
                .as_object()
                .ok_or_else(|| invalid("Некорректный task filter"))?;
            if f.iter().any(|(k, v)| {
                ![
                    "text",
                    "status",
                    "project",
                    "priority",
                    "tag",
                    "scheduled",
                    "expression",
                ]
                .contains(&k.as_str())
                    || !v.as_str().is_some_and(|s| s.len() <= 4096)
            }) {
                return Err(invalid("Неизвестное поле task filter"));
            }
        }
        let mut ids = BTreeSet::new();
        for view in views {
            let id = view
                .get("id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty() && s.len() <= 128)
                .ok_or_else(|| invalid("Некорректный ID task view"))?;
            if !ids.insert(id)
                || !view
                    .get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty() && s.len() <= 128)
                || !matches!(
                    view.get("type").and_then(Value::as_str),
                    Some("brandon-task-table" | "brandon-task-kanban" | "brandon-task-timeline")
                )
            {
                return Err(invalid("Неизвестный или повторяющийся task view"));
            }
            if view
                .get("windowDays")
                .is_some_and(|v| v.as_f64().is_none_or(|n| ![7., 14., 30., 60.].contains(&n)))
            {
                return Err(invalid("Неизвестное окно Timeline"));
            }
        }
    } else if views
        .iter()
        .any(|v| v.get("type").and_then(Value::as_str) != Some("table"))
    {
        return Err(invalid("Поддерживается табличный поднабор note .base"));
    }
    Ok(())
}
fn canvas(value: &Value, files: &BTreeSet<String>, root: &str) -> Result<()> {
    namespace(value.get("brandon"))?;
    namespace(value.pointer("/brandon/presentation"))?;
    let nodes = value
        .get("nodes")
        .and_then(Value::as_array)
        .filter(|v| v.len() <= 2000)
        .ok_or_else(|| invalid("Canvas: до 2000 узлов"))?;
    let edges = value
        .get("edges")
        .and_then(Value::as_array)
        .filter(|v| v.len() <= 4000)
        .ok_or_else(|| invalid("Canvas: до 4000 связей"))?;
    let mut ids = BTreeSet::new();
    let mut ink_points = 0usize;
    for node in nodes {
        let id = node
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 512)
            .ok_or_else(|| invalid("ID Canvas отсутствует или слишком длинный"))?;
        if !ids.insert(id) {
            return Err(invalid("Повторяющийся ID Canvas"));
        }
        if !matches!(
            node.get("type").and_then(Value::as_str),
            Some("text" | "group" | "file" | "link")
        ) {
            return Err(invalid("Неизвестный тип Canvas"));
        }
        for key in ["x", "y", "width", "height"] {
            let n = node
                .get(key)
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite() && n.abs() <= 10000000.)
                .ok_or_else(|| invalid("Некорректная геометрия Canvas"))?;
            if matches!(key, "width" | "height") && n <= 0. {
                return Err(invalid("Нулевой размер Canvas"));
            }
        }
        namespace(node.get("brandon"))?;
        ink_points += node_extension(node)?;
        if ink_points > 100000 {
            return Err(invalid("Слишком много точек штрихов в шаблоне"));
        }
        for key in ["text", "file", "url", "label", "color"] {
            if node.get(key).is_some_and(|v| !v.is_string()) {
                return Err(invalid("Некорректное строковое поле Canvas"));
            }
        }
        namespace(node.pointer("/brandon/frame"))?;
        namespace(node.pointer("/brandon/ink"))?;
        if node.pointer("/brandon/taskId").is_some() {
            return Err(invalid(
                "Шаблон не переносит существующий TaskId. Используйте связанные task views и создайте новую задачу явно.",
            ));
        }
        if let Some(path) = node.get("file").and_then(Value::as_str)
            && !files.contains(path)
        {
            return Err(invalid(
                "Canvas ссылается на файл вне создаваемого пространства",
            ));
        }
        if node.get("type").and_then(Value::as_str) == Some("link")
            && !node
                .get("url")
                .and_then(Value::as_str)
                .is_some_and(|u| u.starts_with("https://") || u.starts_with("http://"))
        {
            return Err(invalid(
                "Canvas URL должен быть обычной http(s)-ссылкой, не исполняемой схемой",
            ));
        }
    }
    let mut edge_ids = BTreeSet::new();
    for edge in edges {
        let id = edge
            .get("id")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty() && s.len() <= 512)
            .ok_or_else(|| invalid("Некорректный ID связи"))?;
        if !edge_ids.insert(id)
            || !["fromNode", "toNode"].iter().all(|k| {
                edge.get(k)
                    .and_then(Value::as_str)
                    .is_some_and(|v| ids.contains(v))
            })
        {
            return Err(invalid("Повторяющаяся или висячая связь шаблона"));
        }
        namespace(edge.get("brandon"))?;
        for key in ["fromSide", "toSide"] {
            if edge
                .get(key)
                .is_some_and(|v| !matches!(v.as_str(), Some("top" | "right" | "bottom" | "left")))
            {
                return Err(invalid("Некорректная сторона связи"));
            }
        }
        for key in ["fromEnd", "toEnd"] {
            if edge
                .get(key)
                .is_some_and(|v| !matches!(v.as_str(), Some("none" | "arrow")))
            {
                return Err(invalid("Некорректный конец связи"));
            }
        }
        if edge
            .pointer("/brandon/dash")
            .is_some_and(|v| !matches!(v.as_str(), Some("solid" | "dashed")))
            || edge
                .pointer("/brandon/strokeWidth")
                .is_some_and(|v| v.as_f64().is_none_or(|n| ![1., 2., 4.].contains(&n)))
        {
            return Err(invalid("Неизвестный стиль связи"));
        }
    }
    let _ = root;
    Ok(())
}
pub(super) fn render(
    template: &Template,
    input: &TemplateInput,
    root: &str,
) -> Result<(Vec<TemplateOutput>, Vec<String>)> {
    let paths: BTreeSet<String> = template
        .files
        .iter()
        .map(|f| format!("{root}/{}", f.path))
        .collect();
    let mut out = Vec::new();
    let mut total = 0usize;
    let mut warnings = Vec::new();
    for TemplateFile { path, content, .. } in &template.files {
        let lower_path = path.to_lowercase();
        let text = if lower_path.ends_with(".md") {
            substitute(content, input, root)?
        } else {
            json_source(content, input, root)?
        };
        total += text.len();
        if total > 8 * 1024 * 1024 {
            return Err(invalid("Состав проекта превышает 8 МиБ"));
        }
        if lower_path.ends_with(".md") {
            let lower = text.to_lowercase();
            if [
                "<script", "<iframe", "<img", "<object", "<embed", "<video", "<audio", "<link",
                "<style",
            ]
            .iter()
            .any(|tag| lower.contains(tag))
            {
                return Err(invalid(
                    "Активный HTML/медиа не являются возможностью шаблона",
                ));
            }
            for link in knowledge_markdown::parse_document(path, &text, 0).links {
                if link.kind == knowledge_markdown::LinkKind::Embed {
                    return Err(invalid(
                        "Шаблоны не загружают вложенные источники/изображения автоматически",
                    ));
                }
                if link.target.starts_with("https://") || link.target.starts_with("http://") {
                    if warnings.len() < 128 {
                        warnings.push(format!("{path}: внешняя ссылка остаётся текстом в предпросмотре; открытие отдельно не является разрешением сети для агента"))
                    }
                } else if !local_target(path, &link.target, root, &paths) {
                    return Err(invalid(
                        "Markdown-ссылка ведёт за пределы создаваемого состава",
                    ));
                }
            }
        } else {
            let value: Value =
                serde_json::from_str(&text).map_err(|_| invalid("Результат JSON некорректен"))?;
            if lower_path.ends_with(".canvas") {
                canvas(&value, &paths, root)?;
                if value
                    .get("nodes")
                    .and_then(Value::as_array)
                    .is_some_and(|nodes| {
                        nodes
                            .iter()
                            .any(|n| n.get("type") == Some(&Value::String("link".into())))
                    })
                {
                    warnings.push(format!("{path}: web-карточки не загружаются при создании"))
                }
            } else {
                base(&value)?
            }
        }
        out.push(TemplateOutput {
            path: path.clone(),
            fingerprint: fingerprint(text.as_bytes()),
            content: text,
        });
    }
    Ok((out, warnings))
}
