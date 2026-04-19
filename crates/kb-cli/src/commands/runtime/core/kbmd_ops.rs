fn parse_categories(kb_md: &Path) -> Result<Vec<String>, CliError> {
    parse_kb_document_file(kb_md).map(|doc| doc.categories)
}

fn append_to_kb_md(
    kb_md: &Path,
    category: &str,
    alias: &str,
    path: &str,
) -> Result<(), std::io::Error> {
    let mut content = fs::read_to_string(kb_md).unwrap_or_default();
    let has_category = parse_kb_document(&content)
        .ok()
        .map(|doc| doc.categories.iter().any(|x| x == category))
        .unwrap_or(false);

    if !has_category {
        if !content.ends_with('\n') && !content.is_empty() {
            content.push('\n');
        }
        content.push_str(&format!("# {category}\n"));
    }
    if !content.ends_with('\n') && !content.is_empty() {
        content.push('\n');
    }
    content.push_str(&format!("[{alias}]({path})\n"));
    fs::write(kb_md, content)
}

fn append_missing_raw_refs_to_kb_md(
    cwd: &Path,
    kb_doc: &crate::kb_md::KbDocument,
    desired: &[DesiredEntry],
) -> Result<usize, std::io::Error> {
    let kb_md = cwd.join("kb.md");
    let mut already_referenced = kb_doc
        .references
        .iter()
        .map(|r| normalize_path(&r.path))
        .collect::<HashSet<_>>();

    let mut appended = 0usize;
    for item in desired {
        if item.source != "raw" || !item.exists {
            continue;
        }
        if already_referenced.contains(&item.path) {
            continue;
        }
        append_to_kb_md(&kb_md, "raw", "", &item.path)?;
        already_referenced.insert(item.path.clone());
        appended += 1;
    }
    Ok(appended)
}

fn apply_rm_to_kb_md(
    kb_md: &Path,
    kb_doc: &crate::kb_md::KbDocument,
    target: &str,
) -> Result<Option<(String, String)>, std::io::Error> {
    let raw = fs::read_to_string(kb_md)?;
    let mut lines = raw.lines().map(ToString::to_string).collect::<Vec<_>>();
    let reference = resolve_rm_target(kb_doc, target);

    let Some(reference) = reference else {
        return Ok(None);
    };

    let idx = reference.line.saturating_sub(1);
    if idx >= lines.len() {
        return Ok(None);
    }
    lines.remove(idx);

    let mut out = lines.join("\n");
    if raw.ends_with('\n') {
        out.push('\n');
    }
    fs::write(kb_md, out)?;

    Ok(Some(reference.identity()))
}

#[derive(Debug, Clone)]
struct RmTargetRef {
    line: usize,
    alias: String,
    path: String,
}

impl RmTargetRef {
    fn identity(&self) -> (String, String) {
        (self.alias.clone(), self.path.clone())
    }
}

fn resolve_rm_target(kb_doc: &crate::kb_md::KbDocument, target: &str) -> Option<RmTargetRef> {
    let normalized_target = normalize_path(target);
    kb_doc.references.iter().find_map(|item| {
        let normalized_ref_path = normalize_path(&item.path);
        let id = item
            .alias
            .clone()
            .unwrap_or_else(|| normalized_ref_path.clone());
        if item.alias.as_deref() == Some(target)
            || id == target
            || normalized_ref_path == normalized_target
        {
            Some(RmTargetRef {
                line: item.line,
                alias: item.alias.clone().unwrap_or_else(|| normalized_ref_path.clone()),
                path: normalized_ref_path,
            })
        } else {
            None
        }
    })
}

fn apply_chmod_to_kb_md(
    kb_md: &Path,
    kb_doc: &crate::kb_md::KbDocument,
    target: &str,
    perms: &str,
) -> Result<bool, std::io::Error> {
    let raw = fs::read_to_string(kb_md)?;
    let mut lines = raw.lines().map(ToString::to_string).collect::<Vec<_>>();
    let normalized_target = normalize_path(target);

    if let Some(reference) = kb_doc.references.iter().find(|item| {
        let normalized_ref_path = normalize_path(&item.path);
        let id = item
            .alias
            .clone()
            .unwrap_or_else(|| normalized_ref_path.clone());
        item.alias.as_deref() == Some(target) || id == target || normalized_ref_path == normalized_target
    }) {
        let idx = reference.line.saturating_sub(1);
        if idx < lines.len() {
            let old_line = lines[idx].clone();
            let leading_ws = old_line
                .chars()
                .take_while(|ch| ch.is_whitespace())
                .collect::<String>();
            let comment = old_line
                .find('#')
                .map(|pos| old_line[pos..].trim().to_string());
            let alias_text = reference.alias.clone().unwrap_or_default();
            let mut new_line = format!("{leading_ws}[{alias_text}]({}):{perms}", reference.path);
            if let Some(comment_text) = comment {
                if !comment_text.is_empty() {
                    new_line.push(' ');
                    new_line.push_str(&comment_text);
                }
            }
            lines[idx] = new_line;
            let mut out = lines.join("\n");
            if raw.ends_with('\n') {
                out.push('\n');
            }
            fs::write(kb_md, out)?;
            return Ok(true);
        }
    }

    for idx in 0..lines.len() {
        if let Some((level, name, leading_ws)) = parse_heading_line_for_chmod(&lines[idx]) {
            if name == target {
                lines[idx] = format!("{leading_ws}{} {name}:{perms}", "#".repeat(level));
                let mut out = lines.join("\n");
                if raw.ends_with('\n') {
                    out.push('\n');
                }
                fs::write(kb_md, out)?;
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn parse_heading_line_for_chmod(line: &str) -> Option<(usize, String, String)> {
    let leading_ws = line
        .chars()
        .take_while(|ch| ch.is_whitespace())
        .collect::<String>();
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if level == 0 {
        return None;
    }
    let raw_title = trimmed[level..].trim();
    if raw_title.is_empty() {
        return None;
    }
    if let Some((lhs, rhs)) = raw_title.rsplit_once(':') {
        let maybe_perm = rhs.trim();
        if !maybe_perm.is_empty() && Permission::parse(maybe_perm).is_ok() {
            return Some((level, lhs.trim().to_string(), leading_ws));
        }
    }
    Some((level, raw_title.to_string(), leading_ws))
}

fn prune_missing_refs_from_kb_md(
    cwd: &Path,
    kb_doc: &crate::kb_md::KbDocument,
    diff: &RegistryDiff,
) -> Result<usize, std::io::Error> {
    let targets = diff
        .to_remove
        .iter()
        .filter(|item| item.reason == "not_found")
        .map(|item| item.path.clone())
        .collect::<HashSet<_>>();
    if targets.is_empty() {
        return Ok(0);
    }

    let kb_md = cwd.join("kb.md");
    let raw = fs::read_to_string(&kb_md)?;
    let mut lines = raw.lines().map(ToString::to_string).collect::<Vec<_>>();
    let mut removed = 0usize;

    let mut remove_indexes = kb_doc
        .references
        .iter()
        .filter(|item| targets.contains(&normalize_path(&item.path)))
        .map(|item| item.line.saturating_sub(1))
        .collect::<Vec<_>>();
    remove_indexes.sort_unstable();
    remove_indexes.dedup();
    remove_indexes.reverse();

    for idx in remove_indexes {
        if idx < lines.len() {
            lines.remove(idx);
            removed += 1;
        }
    }

    if removed > 0 {
        let mut out = lines.join("\n");
        if raw.ends_with('\n') {
            out.push('\n');
        }
        fs::write(kb_md, out)?;
    }

    Ok(removed)
}

fn dedupe_raw_refs_in_kb_md(
    cwd: &Path,
    kb_doc: &crate::kb_md::KbDocument,
) -> Result<usize, std::io::Error> {
    let kb_md = cwd.join("kb.md");
    let raw = fs::read_to_string(&kb_md)?;
    let mut lines = raw.lines().map(ToString::to_string).collect::<Vec<_>>();
    let mut seen = HashSet::new();
    let mut remove_indexes = Vec::new();

    for item in &kb_doc.references {
        let normalized_path = normalize_path(&item.path);
        if !normalized_path.starts_with(RAW_PREFIX) {
            continue;
        }
        if !seen.insert(normalized_path) {
            remove_indexes.push(item.line.saturating_sub(1));
        }
    }

    if remove_indexes.is_empty() {
        return Ok(0);
    }

    remove_indexes.sort_unstable();
    remove_indexes.dedup();
    remove_indexes.reverse();

    let mut removed = 0usize;
    for idx in remove_indexes {
        if idx < lines.len() {
            lines.remove(idx);
            removed += 1;
        }
    }

    if removed > 0 {
        let mut out = lines.join("\n");
        if raw.ends_with('\n') {
            out.push('\n');
        }
        fs::write(kb_md, out)?;
    }

    Ok(removed)
}

fn project_name(cwd: &Path) -> String {
    cwd.file_name()
        .and_then(|x| x.to_str())
        .unwrap_or("demo")
        .to_string()
}

fn to_change_json(limit: usize, items: &[CheckResult]) -> Vec<Value> {
    items
        .iter()
        .take(limit)
        .map(|x| json!({"alias": x.alias, "path": x.path, "reason": x.reason}))
        .collect()
}

fn to_add_json(limit: usize, items: &[CheckResult]) -> Vec<Value> {
    items
        .iter()
        .take(limit)
        .map(|x| {
            json!({
                "alias": x.alias,
                "path": x.path,
                "source": if x.path.starts_with(RAW_PREFIX) { "raw" } else { "kbmd" }
            })
        })
        .collect()
}

fn metadata_summary(abs: &Path) -> (u64, String) {
    match fs::metadata(abs) {
        Ok(meta) => {
            let size = meta.len();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|x| x.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|x| x.as_secs().to_string())
                .unwrap_or_else(|| "0".to_string());
            (size, mtime)
        }
        Err(_) => (0, "0".to_string()),
    }
}
