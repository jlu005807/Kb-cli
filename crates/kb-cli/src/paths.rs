use std::fs;
use std::path::Path;

// raw 目录策略统一放在这里，避免业务代码到处硬编码路径。
pub const RAW_DIR_REL: &str = "./.kb/raw";
pub const RAW_PREFIX: &str = "./.kb/raw/";
pub const KBIGNORE_REL: &str = ".kb/.kbignore";

pub fn load_ignore_rules(cwd: &Path) -> Vec<String> {
    let path = cwd.join(KBIGNORE_REL);
    let Ok(raw) = fs::read_to_string(path) else {
        return vec![];
    };
    raw.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(normalize_path)
        .collect()
}

pub fn is_ignored(rules: &[String], path: &str) -> bool {
    // 目录规则以 '/' 结尾，按前缀匹配；其他规则走简化 glob。
    rules.iter().any(|rule| {
        if rule.ends_with('/') {
            let prefix = rule.trim_end_matches('/');
            return path.starts_with(prefix);
        }
        glob_like_match(rule, path)
    })
}

pub fn normalize_path(path: &str) -> String {
    // 归一化目标：
    // 1) 统一分隔符为 '/'
    // 2) 压缩重复分隔符
    // 3) 相对路径统一带 './' 前缀，便于稳定比较
    let mut out = path.replace('\\', "/");
    while out.contains("//") {
        out = out.replace("//", "/");
    }
    while out.starts_with("./") {
        out = out[2..].to_string();
    }

    let is_absolute = out.starts_with('/') || (out.len() >= 2 && out.chars().nth(1) == Some(':'));

    if out == "." {
        out = "".to_string();
    }

    if !is_absolute && !out.is_empty() {
        out = format!("./{out}");
    }
    if out.is_empty() {
        out = "./".to_string();
    }
    out
}

fn glob_like_match(pattern: &str, candidate: &str) -> bool {
    // 轻量 glob 匹配：支持 '*' 与 '?'，用于 MVP 忽略规则。
    let pattern = pattern.as_bytes();
    let text = candidate.as_bytes();
    let (mut p, mut t) = (0usize, 0usize);
    let mut star_idx: Option<usize> = None;
    let mut match_idx = 0usize;

    while t < text.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == text[t]) {
            p += 1;
            t += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star_idx = Some(p);
            match_idx = t;
            p += 1;
        } else if let Some(star) = star_idx {
            p = star + 1;
            match_idx += 1;
            t = match_idx;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

#[cfg(test)]
mod tests {
    use super::normalize_path;

    #[test]
    fn test_normalize_path_cross_platform() {
        assert_eq!(normalize_path("/usr/local/bin"), "/usr/local/bin");
        assert_eq!(normalize_path("/usr//local///bin"), "/usr/local/bin");
        assert_eq!(normalize_path("././docs/a.md"), "./docs/a.md");
        assert_eq!(normalize_path("docs/b.md"), "./docs/b.md");
        assert_eq!(normalize_path("C:\\foo\\bar"), "C:/foo/bar");
        assert_eq!(normalize_path("d:\\foo//bar"), "d:/foo/bar");
        assert_eq!(normalize_path("."), "./");
        assert_eq!(normalize_path(""), "./");
    }
}
