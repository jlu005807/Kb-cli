use kb_core::domain::Permission;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KbFrontMatter {
    pub kb: Option<String>,
    pub agent: Option<String>,
    pub default_perm: String,
}

impl Default for KbFrontMatter {
    fn default() -> Self {
        Self {
            kb: None,
            agent: None,
            default_perm: "r".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KbReference {
    pub alias: Option<String>,
    pub path: String,
    pub perms: String,
    pub category: Option<String>,
    pub line: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KbDocument {
    pub front_matter: KbFrontMatter,
    pub categories: Vec<String>,
    pub headings: Vec<HeadingInfo>,
    pub references: Vec<KbReference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadingInfo {
    pub line: usize,
    pub level: usize,
    pub name: String,
}

#[derive(Debug, Clone)]
struct HeadingScope {
    level: usize,
    name: String,
    perms: String,
}

#[derive(Debug, Clone)]
struct ParsedReference {
    alias: Option<String>,
    path: String,
    explicit_perm: Option<String>,
}

pub fn parse_kb_document(raw: &str) -> Result<KbDocument, String> {
    let raw = raw.strip_prefix('\u{feff}').unwrap_or(raw);
    let lines = raw.lines().collect::<Vec<_>>();
    let (front_matter, body_start_line) = parse_front_matter(&lines)?;

    let mut categories: Vec<String> = Vec::new();
    let mut headings: Vec<HeadingInfo> = Vec::new();
    let mut references: Vec<KbReference> = Vec::new();
    let mut scopes: Vec<HeadingScope> = Vec::new();

    for (idx, line) in lines.iter().enumerate().skip(body_start_line) {
        let line_number = idx + 1;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((level, heading_name, heading_perm)) = parse_heading(trimmed) {
            headings.push(HeadingInfo {
                line: line_number,
                level,
                name: heading_name.clone(),
            });
            while scopes
                .last()
                .map(|scope| scope.level >= level)
                .unwrap_or(false)
            {
                scopes.pop();
            }

            let inherited = scopes
                .last()
                .map(|scope| scope.perms.clone())
                .unwrap_or_else(|| front_matter.default_perm.clone());
            let resolved_perm = heading_perm.unwrap_or(inherited);

            if !categories.iter().any(|x| x == &heading_name) {
                categories.push(heading_name.clone());
            }
            scopes.push(HeadingScope {
                level,
                name: heading_name,
                perms: resolved_perm,
            });
            continue;
        }

        let inherited = scopes
            .last()
            .map(|scope| scope.perms.as_str())
            .unwrap_or(front_matter.default_perm.as_str());

        let Some(parsed) = parse_reference_line(trimmed)? else {
            continue;
        };

        let perms = parsed
            .explicit_perm
            .unwrap_or_else(|| inherited.to_string());
        if Permission::parse(&perms).is_err() {
            return Err(format!(
                "invalid permission at line {}: {}",
                line_number, perms
            ));
        }

        references.push(KbReference {
            alias: parsed.alias,
            path: parsed.path,
            perms,
            category: scopes.last().map(|scope| scope.name.clone()),
            line: line_number,
        });
    }

    Ok(KbDocument {
        front_matter,
        categories,
        headings,
        references,
    })
}

fn parse_front_matter(lines: &[&str]) -> Result<(KbFrontMatter, usize), String> {
    if lines.first().map(|line| line.trim()) != Some("---") {
        return Ok((KbFrontMatter::default(), 0));
    }

    let mut fm = KbFrontMatter::default();
    let mut idx = 1usize;
    while idx < lines.len() {
        let current = lines[idx].trim();
        if current == "---" {
            if Permission::parse(&fm.default_perm).is_err() {
                return Err(format!(
                    "invalid front matter default_perm: {}",
                    fm.default_perm
                ));
            }
            return Ok((fm, idx + 1));
        }
        if !current.is_empty() {
            let Some((key, value)) = current.split_once(':') else {
                return Err(format!(
                    "invalid front matter line {}: {}",
                    idx + 1,
                    current
                ));
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "kb" => fm.kb = Some(value.to_string()),
                "agent" => fm.agent = Some(value.to_string()),
                "default_perm" => fm.default_perm = value.to_string(),
                _ => {}
            }
        }
        idx += 1;
    }

    Err("unterminated front matter".to_string())
}

fn parse_heading(trimmed: &str) -> Option<(usize, String, Option<String>)> {
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if level == 0 {
        return None;
    }

    let raw_title = trimmed[level..].trim();
    if raw_title.is_empty() {
        return None;
    }

    let (name, explicit_perm) = split_title_and_permission(raw_title);
    if name.is_empty() {
        return None;
    }

    Some((level, name, explicit_perm))
}

fn split_title_and_permission(title: &str) -> (String, Option<String>) {
    if let Some((lhs, rhs)) = title.rsplit_once(':') {
        let maybe_perm = rhs.trim();
        if !maybe_perm.is_empty() && Permission::parse(maybe_perm).is_ok() {
            return (lhs.trim().to_string(), Some(maybe_perm.to_string()));
        }
    }
    (title.trim().to_string(), None)
}

fn parse_reference_line(trimmed: &str) -> Result<Option<ParsedReference>, String> {
    if let Some(item) = parse_markdown_reference(trimmed)? {
        return Ok(Some(item));
    }
    if let Some(item) = parse_legacy_reference(trimmed)? {
        return Ok(Some(item));
    }
    Ok(None)
}

fn parse_markdown_reference(trimmed: &str) -> Result<Option<ParsedReference>, String> {
    if !trimmed.starts_with('[') {
        return Ok(None);
    }

    let Some(alias_end) = trimmed.find(']') else {
        return Err(format!("invalid reference syntax: {}", trimmed));
    };
    let alias_raw = trimmed[1..alias_end].trim();
    let after_alias = trimmed[alias_end + 1..].trim_start();
    if !after_alias.starts_with('(') {
        return Err(format!("invalid reference syntax: {}", trimmed));
    }

    let Some(path_end_offset) = after_alias[1..].find(')') else {
        return Err(format!("invalid reference syntax: {}", trimmed));
    };
    let path_end = 1 + path_end_offset;
    let path_raw = after_alias[1..path_end].trim();
    if path_raw.is_empty() {
        return Err("reference path cannot be empty".to_string());
    }

    let tail = after_alias[path_end + 1..].trim_start();
    let tail_without_comment = tail
        .split_once('#')
        .map(|(head, _)| head.trim())
        .unwrap_or(tail);

    let explicit_perm = if tail_without_comment.is_empty() {
        None
    } else if let Some(rest) = tail_without_comment.strip_prefix(':') {
        let perms = rest.trim();
        if perms.is_empty() {
            return Err(format!("invalid permission declaration: {}", trimmed));
        }
        Some(perms.to_string())
    } else {
        return Err(format!("invalid reference tail: {}", trimmed));
    };

    Ok(Some(ParsedReference {
        alias: if alias_raw.is_empty() {
            None
        } else {
            Some(alias_raw.to_string())
        },
        path: path_raw.to_string(),
        explicit_perm,
    }))
}

fn parse_legacy_reference(trimmed: &str) -> Result<Option<ParsedReference>, String> {
    if !trimmed.starts_with("- ") {
        return Ok(None);
    }

    let body = trimmed[2..].trim();
    let Some((alias_raw, path_raw)) = body.split_once('|') else {
        return Ok(None);
    };

    let path = path_raw.trim();
    if path.is_empty() {
        return Err("legacy reference path cannot be empty".to_string());
    }

    let alias = alias_raw.trim();
    Ok(Some(ParsedReference {
        alias: if alias.is_empty() {
            None
        } else {
            Some(alias.to_string())
        },
        path: path.to_string(),
        explicit_perm: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::parse_kb_document;

    #[test]
    fn parses_front_matter_and_permission_inheritance() {
        let raw = r#"---
kb: sample
default_perm: rw
---

# docs:r
[guide](docs/guide.md)
## api
[spec](docs/api/spec.md):rw
"#;
        let doc = parse_kb_document(raw).expect("parse kb.md");
        assert_eq!(doc.front_matter.kb.as_deref(), Some("sample"));
        assert_eq!(doc.front_matter.default_perm, "rw");
        assert_eq!(doc.references.len(), 2);
        assert_eq!(doc.references[0].perms, "r");
        assert_eq!(doc.references[1].perms, "rw");
    }

    #[test]
    fn bom_front_matter_preserves_default_permission() {
        let raw = "\u{feff}---\ndefault_perm: rw\n---\n# docs\n[guide](docs/guide.md)\n";
        let doc = parse_kb_document(raw).expect("parse BOM-prefixed kb.md");

        assert_eq!(doc.front_matter.default_perm, "rw");
        assert_eq!(doc.references[0].perms, "rw");
    }

    #[test]
    fn bom_prefixed_first_line_is_recognized_as_heading_without_shifting_lines() {
        let raw = "\u{feff}# docs\n[guide](docs/guide.md)\n";
        let doc = parse_kb_document(raw).expect("parse BOM-prefixed kb.md");

        assert_eq!(doc.categories, vec!["docs"]);
        assert_eq!(doc.references[0].category.as_deref(), Some("docs"));
        assert_eq!(doc.references[0].line, 2);
    }

    #[test]
    fn allows_empty_alias_and_falls_back_to_path_identifier() {
        let raw = r#"
# raw:r
[](./.kb/raw/a.md)
"#;
        let doc = parse_kb_document(raw).expect("parse kb.md");
        assert_eq!(doc.references.len(), 1);
        assert!(doc.references[0].alias.is_none());
        assert_eq!(doc.references[0].path, "./.kb/raw/a.md");
    }
}
