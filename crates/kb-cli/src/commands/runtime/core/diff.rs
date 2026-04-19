fn compute_registry_diff(
    cwd: &Path,
    regs: &[RegistryEntry],
    desired: &[DesiredEntry],
    baseline: Option<&IndexSnapshot>,
) -> RegistryDiff {
    let mut diff = RegistryDiff::default();

    let regs_by_path: BTreeMap<&str, &RegistryEntry> =
        regs.iter().map(|item| (item.path.as_str(), item)).collect();
    let desired_by_path: BTreeMap<&str, &DesiredEntry> =
        desired.iter().map(|item| (item.path.as_str(), item)).collect();

    let baseline_signature: BTreeMap<&str, &str> = baseline
        .map(|snapshot| {
            snapshot
                .records
                .iter()
                .filter(|record| record.present)
                .map(|record| (record.path.as_str(), record.signature.as_str()))
                .collect()
        })
        .unwrap_or_default();

    let mut removed_paths = HashSet::new();
    for item in desired {
        if !item.exists {
            diff.to_remove.push(CheckResult {
                alias: item.id.clone(),
                path: item.path.clone(),
                reason: "not_found".to_string(),
            });
            if regs_by_path.contains_key(item.path.as_str()) {
                removed_paths.insert(item.path.clone());
            }
            continue;
        }

        match regs_by_path.get(item.path.as_str()) {
            None => {
                diff.to_add.push(CheckResult {
                    alias: item.id.clone(),
                    path: item.path.clone(),
                    reason: "new_path".to_string(),
                });
                diff.add_entries.push(item.to_registry_entry());
            }
            Some(existing) => {
                let metadata_changed = existing.id != item.id
                    || existing.alias != item.alias
                    || existing.perms != item.perms
                    || existing.source != item.source
                    || existing.category != item.category;
                if metadata_changed {
                    diff.to_update.push(CheckResult {
                        alias: item.id.clone(),
                        path: item.path.clone(),
                        reason: "metadata_changed".to_string(),
                    });
                    diff.update_entries.push(item.to_registry_entry());
                    continue;
                }

                let current_sig = compute_signature(&cwd.join(item.path.trim_start_matches("./")));
                if baseline_signature
                    .get(item.path.as_str())
                    .map(|sig| **sig != current_sig)
                    .unwrap_or(false)
                {
                    diff.to_update.push(CheckResult {
                        alias: item.id.clone(),
                        path: item.path.clone(),
                        reason: "content_changed".to_string(),
                    });
                }
            }
        }
    }

    for reg in regs {
        if !desired_by_path.contains_key(reg.path.as_str()) {
            diff.to_remove.push(CheckResult {
                alias: reg.id.clone(),
                path: reg.path.clone(),
                reason: "deleted".to_string(),
            });
            removed_paths.insert(reg.path.clone());
        }
    }

    diff.to_add
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.alias.cmp(&b.alias)));
    diff.to_update
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.alias.cmp(&b.alias)));
    diff.to_remove
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.alias.cmp(&b.alias)));

    let mut removed = removed_paths.into_iter().collect::<Vec<_>>();
    removed.sort();
    diff.removed_paths = removed;

    diff
}

fn apply_registry_diff(registry: &mut RegistryState, diff: &RegistryDiff) {
    let removed = diff.removed_paths.iter().cloned().collect::<HashSet<_>>();
    registry.regs.retain(|item| !removed.contains(&item.path));

    for item in &diff.update_entries {
        if let Some(existing) = registry.regs.iter_mut().find(|x| x.path == item.path) {
            *existing = item.clone();
        }
    }

    for item in &diff.add_entries {
        if !registry.regs.iter().any(|x| x.path == item.path) {
            registry.regs.push(item.clone());
        }
    }

    registry
        .regs
        .sort_by(|a, b| a.path.cmp(&b.path).then(a.id.cmp(&b.id)));
}

fn compute_signature(path: &Path) -> String {
    if !path.exists() {
        return "missing".to_string();
    }
    if path.is_file() {
    if let Ok(mut file) = fs::File::open(path) {
            let mut h: u64 = 5381;
            let mut buf = [0u8; 8192];
            loop {
                match file.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        for byte in &buf[..n] {
                            h = ((h << 5).wrapping_add(h)).wrapping_add(*byte as u64);
                        }
                    }
                    Err(_) => return "unknown".to_string(),
                }
            }
            return format!("f:{h}");
        }
    }
    match fs::metadata(path) {
        Ok(meta) => format!("d:{}", meta.len()),
        Err(_) => "unknown".to_string(),
    }
}

fn compute_excerpt(path: &Path) -> Option<String> {
    if !path.exists() || !path.is_file() {
        return None;
    }

    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();

    loop {
        line.clear();
        let read = reader.read_line(&mut line).ok()?;
        if read == 0 {
            return None;
        }
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.chars().take(480).collect());
        }
    }
}

