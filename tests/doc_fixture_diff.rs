use anyhow::{Context, Result};
use regex::Regex;
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::{Path, PathBuf};

#[path = "common/mod.rs"]
mod common;

use common::{extract_json_blocks, normalize_json, read_markdown};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_fixture(path: &Path) -> Result<String> {
    let s = fs::read_to_string(path).with_context(|| format!("read fixture {}", path.display()))?;
    Ok(s.replace("\r\n", "\n"))
}

fn extract_command_section<'a>(md: &'a str, cmd: &str) -> Option<&'a str> {
    let heading_re = Regex::new(r"(?m)^##\s+\d+\.\s+`kb\s+([^\s`]+)[^`]*`").unwrap();

    let mut headings: Vec<(usize, usize, &str)> = Vec::new();
    for cap in heading_re.captures_iter(md) {
        let m = cap.get(0)?;
        let c = cap.get(1)?;
        headings.push((m.start(), m.end(), c.as_str()));
    }

    for (idx, (start, _, heading_cmd)) in headings.iter().enumerate() {
        if *heading_cmd == cmd {
            let end = headings.get(idx + 1).map(|next| next.0).unwrap_or(md.len());
            return Some(&md[*start..end]);
        }
    }

    None
}

#[test]
fn doc_fixture_parity() -> Result<()> {
    let root = workspace_root();
    let md_path = root.join("docs/cli/command-examples.md");
    let md = read_markdown(&md_path.to_string_lossy())?;
    let cmds = vec![
        "init", "add", "mv", "rm", "path", "chmod", "check", "update", "list", "ask",
    ];

    let failure_heading_re = Regex::new(r"###\s+(?:Failure Example|失败示例)").unwrap();

    for cmd in cmds {
        let section = extract_command_section(&md, cmd)
            .map(str::to_owned)
            .unwrap_or_else(|| panic!("section for `{}` not found in command-examples.md", cmd));

        let parts: Vec<&str> = failure_heading_re.split(&section).collect();
        let succ_part = parts[0];
        let fail_part = if parts.len() > 1 { parts[1] } else { "" };

        // Parity contract: first success JSON + first failure JSON per command.
        let succ = extract_json_blocks(succ_part)
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                panic!(
                    "first success json block missing for command `{}` in command-examples.md",
                    cmd
                )
            });
        let fail = extract_json_blocks(fail_part)
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                panic!(
                    "first failure json block missing for command `{}` in command-examples.md",
                    cmd
                )
            });

        for (state, jstr) in [("success", succ), ("failure", fail)] {
            let normalized_doc = normalize_json(&jstr)?;

            let fix_path = root.join(format!("tests/fixtures/cli/{}_{}.json", cmd, state));
            if !fix_path.exists() {
                panic!("Missing fixture {}", fix_path.display());
            }
            let fix = read_fixture(&fix_path)?;
            let normalized_fix = normalize_json(&fix)?;
            if normalized_doc != normalized_fix {
                let diff = TextDiff::from_lines(&normalized_fix, &normalized_doc);
                let mut s = String::new();
                for op in diff.iter_all_changes() {
                    let sign = match op.tag() {
                        ChangeTag::Delete => "-",
                        ChangeTag::Insert => "+",
                        ChangeTag::Equal => " ",
                    };
                    s.push_str(&format!("{}{}", sign, op));
                }
                panic!(
                    "Mismatch for `{}` {} (fixture vs docs, normalized JSON):\n{}",
                    cmd, state, s
                );
            }
        }
    }
    Ok(())
}
