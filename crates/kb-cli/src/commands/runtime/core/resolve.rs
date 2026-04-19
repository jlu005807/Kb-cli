fn parse_kb_document_file(kb_md: &Path) -> Result<crate::kb_md::KbDocument, CliError> {
    let raw = fs::read_to_string(kb_md)
        .map_err(|_| map_core_error(CoreErrorKind::InvalidKbFormat, json!({})))?;
    parse_kb_document(&raw).map_err(|err| {
        map_core_error(
            CoreErrorKind::InvalidKbFormat,
            json!({ "path": normalize_path(&kb_md.to_string_lossy()), "message": err }),
        )
    })
}

fn collect_desired_entries(
    cwd: &Path,
    kb_doc: &crate::kb_md::KbDocument,
    ignored: &BTreeSet<String>,
    raw_only: bool,
    category: Option<&str>,
) -> Result<Vec<DesiredEntry>, CliError> {
    let mut by_path: BTreeMap<String, DesiredEntry> = BTreeMap::new();

    for item in collect_raw_entries(cwd) {
        by_path.insert(item.path.clone(), item);
    }

    for item in collect_kb_entries(cwd, kb_doc)? {
        by_path.insert(item.path.clone(), item);
    }

    let mut out = by_path.into_values().collect::<Vec<_>>();
    out.retain(|entry| {
        category.map(|c| entry.category == c).unwrap_or(true)
            && (!raw_only || entry.path.starts_with(RAW_PREFIX))
            && !ignored.contains(&entry.path)
            && !is_protected_index_target(cwd, &cwd.join(entry.path.trim_start_matches("./")))
    });
    Ok(out)
}

fn collect_raw_entries(cwd: &Path) -> Vec<DesiredEntry> {
    let raw_dir = cwd.join(RAW_DIR_REL.trim_start_matches("./"));
    collect_files_under(cwd, &raw_dir, true)
        .into_iter()
        .map(|path| DesiredEntry {
            id: path.clone(),
            alias: None,
            path: path.clone(),
            perms: "r".to_string(),
            category: "raw".to_string(),
            source: "raw".to_string(),
            exists: true,
        })
        .collect()
}

fn collect_kb_entries(
    cwd: &Path,
    kb_doc: &crate::kb_md::KbDocument,
) -> Result<Vec<DesiredEntry>, CliError> {
    let mut out = Vec::new();

    for r in &kb_doc.references {
        let resolved = resolve_reference_paths(cwd, &r.path);
        if resolved.is_empty() {
            continue;
        }

        let singular_plain = resolved.len() == 1 && !is_pattern_reference(&r.path);
        for (path, exists) in resolved {
            let alias = if singular_plain { r.alias.clone() } else { None };
            let id = alias.clone().unwrap_or_else(|| path.clone());
            out.push(DesiredEntry {
                id,
                alias,
                path: path.clone(),
                perms: r.perms.clone(),
                category: r.category.clone().unwrap_or_else(|| "raw".to_string()),
                source: if path.starts_with(RAW_PREFIX) {
                    "raw".to_string()
                } else {
                    "kbmd".to_string()
                },
                exists,
            });
        }
    }

    Ok(out)
}

fn is_pattern_reference(path_spec: &str) -> bool {
    path_spec.ends_with('/')
        || path_spec.ends_with("/*")
        || path_spec.ends_with("/**")
        || path_spec.contains('*')
        || path_spec.contains('?')
        || path_spec.contains('[')
        || path_spec.contains(']')
        || path_spec.contains('{')
        || path_spec.contains('}')
}

fn resolve_reference_paths(cwd: &Path, path_spec: &str) -> Vec<(String, bool)> {
    let normalized = normalize_path(path_spec);

    if normalized.ends_with("/**") {
        let base = normalized.trim_end_matches("/**");
        let dir = cwd.join(base.trim_start_matches("./"));
        return collect_files_under(cwd, &dir, true)
            .into_iter()
            .map(|path| (path, true))
            .collect();
    }

    if normalized.ends_with("/*") {
        let base = normalized.trim_end_matches("/*");
        let dir = cwd.join(base.trim_start_matches("./"));
        return collect_files_under(cwd, &dir, false)
            .into_iter()
            .map(|path| (path, true))
            .collect();
    }

    if normalized.ends_with('/') {
        let dir = cwd.join(normalized.trim_start_matches("./").trim_end_matches('/'));
        return collect_files_under(cwd, &dir, true)
            .into_iter()
            .map(|path| (path, true))
            .collect();
    }

    if is_pattern_reference(&normalized) {
        return resolve_glob_matches(cwd, &normalized)
            .into_iter()
            .map(|path| (path, true))
            .collect();
    }

    let abs = cwd.join(normalized.trim_start_matches("./"));
    vec![(normalized, abs.exists())]
}

fn resolve_glob_matches(cwd: &Path, pattern: &str) -> Vec<String> {
    let candidates = walk_all_files(cwd);
    let expanded = expand_braces(pattern);
    let mut matched = candidates
        .into_iter()
        .filter(|path| {
            expanded
                .iter()
                .any(|rule| glob_like_extended_match(rule, path))
        })
        .collect::<Vec<_>>();
    matched.sort();
    matched.dedup();
    matched
}

fn walk_all_files(root: &Path) -> Vec<String> {
    collect_files_under(root, root, true)
}

fn collect_files_under(cwd: &Path, dir: &Path, recursive: bool) -> Vec<String> {
    if !dir.exists() || !dir.is_dir() {
        return vec![];
    }

    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if recursive {
                    stack.push(path);
                }
                continue;
            }
            if path.is_file() {
                if let Ok(rel) = path.strip_prefix(cwd) {
                    out.push(normalize_path(&rel.to_string_lossy()));
                }
            }
        }
    }
    out
}

fn expand_braces(pattern: &str) -> Vec<String> {
    let Some((open, close)) = find_first_brace(pattern) else {
        return vec![pattern.to_string()];
    };

    let prefix = &pattern[..open];
    let body = &pattern[open + 1..close];
    let suffix = &pattern[close + 1..];

    let variants = split_brace_variants(body);
    let mut out = Vec::new();
    for variant in variants {
        for expanded_suffix in expand_braces(suffix) {
            out.push(format!("{}{}{}", prefix, variant, expanded_suffix));
        }
    }
    out
}

fn find_first_brace(pattern: &str) -> Option<(usize, usize)> {
    let chars = pattern.char_indices().collect::<Vec<_>>();
    for (idx, ch) in &chars {
        if *ch != '{' {
            continue;
        }
        let mut depth = 0usize;
        for (jdx, inner) in chars.iter().skip_while(|(pos, _)| pos < idx) {
            match *inner {
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return Some((*idx, *jdx));
                    }
                }
                _ => {}
            }
        }
        return None;
    }
    None
}

fn split_brace_variants(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let chars = body.char_indices().collect::<Vec<_>>();
    for (idx, ch) in &chars {
        match *ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(body[start..*idx].to_string());
                start = *idx + 1;
            }
            _ => {}
        }
    }
    out.push(body[start..].to_string());
    out
}

#[derive(Debug, Clone)]
enum GlobClassPart {
    Single(char),
    Range(char, char),
}

#[derive(Debug, Clone)]
enum GlobToken {
    Literal(char),
    Star,
    DoubleStar,
    Question,
    CharClass(Vec<GlobClassPart>),
}

fn glob_like_extended_match(pattern: &str, candidate: &str) -> bool {
    let tokens = parse_glob_tokens(pattern);
    let text = candidate.chars().collect::<Vec<_>>();
    let mut memo: HashMap<(usize, usize), bool> = HashMap::new();
    match_glob_tokens(&tokens, 0, &text, 0, &mut memo)
}

fn parse_glob_tokens(pattern: &str) -> Vec<GlobToken> {
    let chars = pattern.chars().collect::<Vec<_>>();
    let mut i = 0usize;
    let mut out = Vec::new();

    while i < chars.len() {
        match chars[i] {
            '*' => {
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    out.push(GlobToken::DoubleStar);
                    i += 2;
                } else {
                    out.push(GlobToken::Star);
                    i += 1;
                }
            }
            '?' => {
                out.push(GlobToken::Question);
                i += 1;
            }
            '[' => {
                let mut j = i + 1;
                while j < chars.len() && chars[j] != ']' {
                    j += 1;
                }
                if j >= chars.len() {
                    out.push(GlobToken::Literal('['));
                    i += 1;
                } else {
                    let content = chars[i + 1..j].to_vec();
                    out.push(GlobToken::CharClass(parse_char_class(&content)));
                    i = j + 1;
                }
            }
            ch => {
                out.push(GlobToken::Literal(ch));
                i += 1;
            }
        }
    }

    out
}

fn parse_char_class(content: &[char]) -> Vec<GlobClassPart> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < content.len() {
        if i + 2 < content.len() && content[i + 1] == '-' {
            out.push(GlobClassPart::Range(content[i], content[i + 2]));
            i += 3;
        } else {
            out.push(GlobClassPart::Single(content[i]));
            i += 1;
        }
    }
    out
}

fn match_glob_tokens(
    tokens: &[GlobToken],
    token_idx: usize,
    text: &[char],
    text_idx: usize,
    memo: &mut HashMap<(usize, usize), bool>,
) -> bool {
    if let Some(cached) = memo.get(&(token_idx, text_idx)) {
        return *cached;
    }

    let answer = if token_idx == tokens.len() {
        text_idx == text.len()
    } else {
        match &tokens[token_idx] {
            GlobToken::Literal(ch) => {
                text_idx < text.len()
                    && text[text_idx] == *ch
                    && match_glob_tokens(tokens, token_idx + 1, text, text_idx + 1, memo)
            }
            GlobToken::Question => {
                text_idx < text.len()
                    && text[text_idx] != '/'
                    && match_glob_tokens(tokens, token_idx + 1, text, text_idx + 1, memo)
            }
            GlobToken::CharClass(parts) => {
                text_idx < text.len()
                    && text[text_idx] != '/'
                    && class_matches(parts, text[text_idx])
                    && match_glob_tokens(tokens, token_idx + 1, text, text_idx + 1, memo)
            }
            GlobToken::Star => {
                let mut idx = text_idx;
                loop {
                    if match_glob_tokens(tokens, token_idx + 1, text, idx, memo) {
                        break true;
                    }
                    if idx >= text.len() || text[idx] == '/' {
                        break false;
                    }
                    idx += 1;
                }
            }
            GlobToken::DoubleStar => {
                let mut idx = text_idx;
                loop {
                    if match_glob_tokens(tokens, token_idx + 1, text, idx, memo) {
                        break true;
                    }
                    if idx >= text.len() {
                        break false;
                    }
                    idx += 1;
                }
            }
        }
    };

    memo.insert((token_idx, text_idx), answer);
    answer
}

fn class_matches(parts: &[GlobClassPart], ch: char) -> bool {
    parts.iter().any(|part| match part {
        GlobClassPart::Single(x) => *x == ch,
        GlobClassPart::Range(start, end) => *start <= ch && ch <= *end,
    })
}

