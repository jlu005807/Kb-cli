/// kb.md 的程序化改写统一走原子写协议（临时文件 + fsync + rename），
/// 与 .kb/*.json 的落盘保持一致，避免进程中断留下半写入的 kb.md。
/// 错误映射回 io::Error 以保持既有函数签名不变。
fn write_kb_md_atomic(kb_md: &Path, content: &str) -> Result<(), std::io::Error> {
    kb_storage_fs::atomic_write_bytes(kb_md, content.as_bytes())
        .map_err(|err| std::io::Error::other(err.to_string()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct KbMdText {
    bom: bool,
    eol: &'static str,
    lines: Vec<String>,
    trailing_eol: bool,
}

impl Default for KbMdText {
    fn default() -> Self {
        Self {
            bom: false,
            eol: "\n",
            lines: Vec::new(),
            trailing_eol: false,
        }
    }
}

impl KbMdText {
    fn read(path: &Path) -> Result<Self, std::io::Error> {
        match fs::read_to_string(path) {
            Ok(raw) => Ok(Self::from_raw(&raw)),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(err) => Err(err),
        }
    }

    fn from_raw(raw: &str) -> Self {
        let bom = raw.starts_with('\u{feff}');
        let content = raw.strip_prefix('\u{feff}').unwrap_or(raw);
        let eol = match content.find('\n') {
            Some(index) if index > 0 && content.as_bytes()[index - 1] == b'\r' => "\r\n",
            _ => "\n",
        };

        Self {
            bom,
            eol,
            lines: content.lines().map(ToString::to_string).collect(),
            trailing_eol: content.ends_with('\n'),
        }
    }

    fn render(&self) -> String {
        let mut content = String::new();
        if self.bom {
            content.push('\u{feff}');
        }
        for (index, line) in self.lines.iter().enumerate() {
            if index > 0 {
                content.push_str(self.eol);
            }
            content.push_str(line);
        }
        if self.trailing_eol {
            content.push_str(self.eol);
        }
        content
    }

    fn write_atomic(&self, path: &Path) -> Result<(), std::io::Error> {
        write_kb_md_atomic(path, &self.render())
    }
}

fn append_to_kb_md(
    kb_md: &Path,
    category: &str,
    alias: &str,
    path: &str,
) -> Result<(), std::io::Error> {
    let mut content = KbMdText::read(kb_md)?;
    let reference = format!("[{alias}]({path})");
    let Ok(document) = parse_kb_document(&content.render()) else {
        content.lines.push(format!("# {category}"));
        content.lines.push(reference);
        content.trailing_eol = true;
        return content.write_atomic(kb_md);
    };

    if let Some(heading) = document.headings.iter().find(|item| item.name == category) {
        let first_line_after_heading = heading.line.min(content.lines.len());
        let stop = document
            .headings
            .iter()
            .find(|item| item.line > heading.line)
            .map(|item| item.line.saturating_sub(1))
            .unwrap_or(content.lines.len())
            .min(content.lines.len());
        let insert_at = (first_line_after_heading..stop)
            .rfind(|index| !content.lines[*index].trim().is_empty())
            .map(|index| index + 1)
            .unwrap_or(first_line_after_heading);
        content.lines.insert(insert_at, reference);
    } else {
        content.lines.push(format!("# {category}"));
        content.lines.push(reference);
    }
    content.trailing_eol = true;
    content.write_atomic(kb_md)
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
    let mut content = KbMdText::read(kb_md)?;
    let reference = resolve_rm_target(kb_doc, target);

    let Some(reference) = reference else {
        return Ok(None);
    };

    let idx = reference.line.saturating_sub(1);
    if idx >= content.lines.len() {
        return Ok(None);
    }
    content.lines.remove(idx);
    content.write_atomic(kb_md)?;

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

/// kb.md 引用行无法安全承载的路径：')' 会截断 markdown 引用语法，
/// '#' 会被 chmod 的行内注释提取截断，控制字符会破坏行结构。
fn path_declaration_unrepresentable(path: &str) -> bool {
    path.contains(')') || path.contains('#') || path.chars().any(|ch| ch.is_control())
}

/// 在保留缩进、显式权限后缀与行内注释的前提下，替换引用行的 alias/path 部分。
/// 支持 markdown 形式 `[alias](path):perm # comment` 与 legacy 形式 `- alias|path`；
/// 无法识别的行返回 None，由调用方决定回退策略。
fn rewrite_reference_line(
    line: &str,
    new_alias: Option<&str>,
    new_path: Option<&str>,
) -> Option<String> {
    let leading_ws: String = line.chars().take_while(|ch| ch.is_whitespace()).collect();
    let trimmed = &line[leading_ws.len()..];

    if trimmed.starts_with('[') {
        let alias_end = trimmed.find(']')?;
        let after_alias = &trimmed[alias_end + 1..];
        let open_offset = after_alias.find('(')?;
        let path_end = after_alias[open_offset + 1..].find(')')? + open_offset + 1;
        let alias_part = trimmed[1..alias_end].trim();
        let path_part = after_alias[open_offset + 1..path_end].trim();
        let tail = &after_alias[path_end + 1..];
        let alias_out = new_alias.unwrap_or(alias_part);
        let path_out = new_path.unwrap_or(path_part);
        return Some(format!("{leading_ws}[{alias_out}]({path_out}){tail}"));
    }

    if let Some(body) = trimmed.strip_prefix("- ") {
        let (alias_part, path_part) = body.split_once('|')?;
        let alias_out = new_alias.unwrap_or_else(|| alias_part.trim());
        let path_out = new_path.unwrap_or_else(|| path_part.trim());
        return Some(format!("{leading_ws}- {alias_out}|{path_out}"));
    }

    None
}

/// 把 kb.md 中 target 对应引用行的 alias 改名。返回 Some((旧 alias, path)) 表示已改写；
/// target 不在 kb.md 或该行无法安全改写时返回 None（不落盘）。
fn apply_mv_to_kb_md(
    kb_md: &Path,
    kb_doc: &crate::kb_md::KbDocument,
    target: &str,
    new_alias: &str,
) -> Result<Option<(String, String)>, std::io::Error> {
    let Some(reference) = resolve_rm_target(kb_doc, target) else {
        return Ok(None);
    };
    let mut content = KbMdText::read(kb_md)?;
    let idx = reference.line.saturating_sub(1);
    if idx >= content.lines.len() {
        return Ok(None);
    }
    let Some(new_line) = rewrite_reference_line(&content.lines[idx], Some(new_alias), None) else {
        return Ok(None);
    };
    content.lines[idx] = new_line;
    content.write_atomic(kb_md)?;
    Ok(Some(reference.identity()))
}

/// 把 kb.md 中 target 对应引用行的 path 替换为 new_path。返回 Some((alias, 旧 path))。
fn apply_path_to_kb_md(
    kb_md: &Path,
    kb_doc: &crate::kb_md::KbDocument,
    target: &str,
    new_path: &str,
) -> Result<Option<(String, String)>, std::io::Error> {
    let Some(reference) = resolve_rm_target(kb_doc, target) else {
        return Ok(None);
    };
    let mut content = KbMdText::read(kb_md)?;
    let idx = reference.line.saturating_sub(1);
    if idx >= content.lines.len() {
        return Ok(None);
    }
    let Some(new_line) = rewrite_reference_line(&content.lines[idx], None, Some(new_path)) else {
        return Ok(None);
    };
    content.lines[idx] = new_line;
    content.write_atomic(kb_md)?;
    Ok(Some(reference.identity()))
}

fn apply_chmod_to_kb_md(
    kb_md: &Path,
    kb_doc: &crate::kb_md::KbDocument,
    target: &str,
    perms: &str,
) -> Result<bool, std::io::Error> {
    let mut content = KbMdText::read(kb_md)?;
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
        if idx < content.lines.len() {
            let old_line = content.lines[idx].clone();
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
            content.lines[idx] = new_line;
            content.write_atomic(kb_md)?;
            return Ok(true);
        }
    }

    for idx in 0..content.lines.len() {
        if let Some((level, name, leading_ws)) = parse_heading_line_for_chmod(&content.lines[idx]) {
            if name == target {
                content.lines[idx] = format!("{leading_ws}{} {name}:{perms}", "#".repeat(level));
                content.write_atomic(kb_md)?;
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
    let mut content = KbMdText::read(&kb_md)?;
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
        if idx < content.lines.len() {
            content.lines.remove(idx);
            removed += 1;
        }
    }

    if removed > 0 {
        content.write_atomic(&kb_md)?;
    }

    Ok(removed)
}

fn dedupe_raw_refs_in_kb_md(
    cwd: &Path,
    kb_doc: &crate::kb_md::KbDocument,
) -> Result<usize, std::io::Error> {
    let kb_md = cwd.join("kb.md");
    let mut content = KbMdText::read(&kb_md)?;
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
        if idx < content.lines.len() {
            content.lines.remove(idx);
            removed += 1;
        }
    }

    if removed > 0 {
        content.write_atomic(&kb_md)?;
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

#[cfg(test)]
mod kbmd_ops_tests {
    use super::{append_to_kb_md, apply_chmod_to_kb_md, rewrite_reference_line, KbMdText};
    use crate::kb_md::parse_kb_document;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_test_path() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("kbmd-ops-{}-{nonce}", std::process::id()))
            .join("kb.md")
    }

    #[test]
    fn chmod_changes_only_the_target_line_in_crlf_kb_md() {
        let path = unique_test_path();
        fs::create_dir_all(path.parent().expect("test directory"))
            .expect("create test directory");
        let original = "# docs:r\r\n[doc](./a.md):r\r\n# next\r\n";
        fs::write(&path, original.as_bytes()).expect("write kb.md");
        let document = parse_kb_document(original).expect("parse kb.md");

        assert!(apply_chmod_to_kb_md(&path, &document, "doc", "rw").expect("chmod kb.md"));
        let written = fs::read(&path).expect("read kb.md");

        assert_eq!(written, b"# docs:r\r\n[doc](./a.md):rw\r\n# next\r\n");
    }

    #[test]
    fn kbmd_text_round_trips_line_endings_bom_and_trailing_eol() {
        for raw in [
            "# docs\n[doc](./a.md)\n",
            "# docs\r\n[doc](./a.md)\r\n",
            "\u{feff}# docs\r\n[doc](./a.md)\r\n",
            "# docs\n[doc](./a.md)",
        ] {
            let path = unique_test_path();
            fs::create_dir_all(path.parent().expect("test directory"))
                .expect("create test directory");
            fs::write(&path, raw.as_bytes()).expect("write kb.md");

            KbMdText::read(&path)
                .expect("read kb.md")
                .write_atomic(&path)
                .expect("write kb.md");

            assert_eq!(fs::read(&path).expect("read written kb.md"), raw.as_bytes());
        }
    }

    #[test]
    fn kbmd_text_normalizes_mixed_line_endings_to_first_style() {
        let path = unique_test_path();
        fs::create_dir_all(path.parent().expect("test directory"))
            .expect("create test directory");
        fs::write(&path, b"# docs\r\n[doc](./a.md)\n# next\r\n").expect("write kb.md");

        KbMdText::read(&path)
            .expect("read kb.md")
            .write_atomic(&path)
            .expect("write kb.md");

        assert_eq!(
            fs::read(&path).expect("read written kb.md"),
            b"# docs\r\n[doc](./a.md)\r\n# next\r\n"
        );
    }

    #[test]
    fn append_to_kb_md_stays_inside_target_heading_section() {
        let cases = [
            (
                "target section at end",
                "# docs\n[old](./old.md)\n",
                "docs",
                "# docs\n[old](./old.md)\n[new](./new.md)\n",
            ),
            (
                "target section in middle",
                "# docs\n[old](./old.md)\n# notes\n[other](./other.md)\n",
                "docs",
                "# docs\n[old](./old.md)\n[new](./new.md)\n# notes\n[other](./other.md)\n",
            ),
            (
                "duplicate category uses first heading",
                "# docs\n[first](./first.md)\n# docs\n[second](./second.md)\n",
                "docs",
                "# docs\n[first](./first.md)\n[new](./new.md)\n# docs\n[second](./second.md)\n",
            ),
            (
                "target section with child heading",
                "# docs\n[old](./old.md)\n## child\n[child](./child.md)\n",
                "docs",
                "# docs\n[old](./old.md)\n[new](./new.md)\n## child\n[child](./child.md)\n",
            ),
            (
                "empty target section",
                "# docs\n# notes\n[other](./other.md)\n",
                "docs",
                "# docs\n[new](./new.md)\n# notes\n[other](./other.md)\n",
            ),
            (
                "target section with trailing blank lines",
                "# docs\n[old](./old.md)\n\n\n# notes\n",
                "docs",
                "# docs\n[old](./old.md)\n[new](./new.md)\n\n\n# notes\n",
            ),
            ("empty file", "", "docs", "# docs\n[new](./new.md)\n"),
        ];

        for (name, original, category, expected) in cases {
            let path = unique_test_path();
            fs::create_dir_all(path.parent().expect("test directory"))
                .expect("create test directory");
            fs::write(&path, original).expect("write kb.md");

            append_to_kb_md(&path, category, "new", "./new.md").expect("append reference");

            assert_eq!(
                fs::read_to_string(path).expect("read updated kb.md"),
                expected,
                "wrong insertion point for {name}"
            );
        }
    }

    #[test]
    fn rewrite_alias_preserves_perm_suffix_and_comment() {
        let line = "  [old](./a.md):rw # keep me";
        let out = rewrite_reference_line(line, Some("new"), None).expect("rewrite");
        assert_eq!(out, "  [new](./a.md):rw # keep me");
    }

    #[test]
    fn rewrite_path_preserves_alias_and_tail() {
        let line = "[doc](./a.md):r";
        let out = rewrite_reference_line(line, None, Some("./b.md")).expect("rewrite");
        assert_eq!(out, "[doc](./b.md):r");
    }

    #[test]
    fn rewrite_gives_alias_to_path_only_reference() {
        let line = "[](./.kb/raw/x.md)";
        let out = rewrite_reference_line(line, Some("raw-x"), None).expect("rewrite");
        assert_eq!(out, "[raw-x](./.kb/raw/x.md)");
    }

    #[test]
    fn rewrite_supports_legacy_pipe_form() {
        let line = "- old|./a.md";
        let out = rewrite_reference_line(line, Some("new"), Some("./b.md")).expect("rewrite");
        assert_eq!(out, "- new|./b.md");
    }

    #[test]
    fn rewrite_rejects_non_reference_lines() {
        assert!(rewrite_reference_line("# heading", Some("x"), None).is_none());
        assert!(rewrite_reference_line("plain text", None, Some("./b.md")).is_none());
    }
}
