use serde_json::Value;
use std::fs;

pub fn read_markdown(path: &str) -> anyhow::Result<String> {
    let s = fs::read_to_string(path)?;
    Ok(s.replace("\r\n", "\n"))
}

pub fn extract_json_blocks(section: &str) -> Vec<String> {
    // extract all ```json ... ``` fenced blocks in-order
    let re = regex::Regex::new(r"(?s)```json[ \t]*\r?\n(.*?)\r?\n```").unwrap();
    re.captures_iter(section)
        .filter_map(|cap| cap.get(1))
        .map(|m| format!("{}\n", m.as_str().replace("\r\n", "\n")))
        .collect()
}

pub fn normalize_json(s: &str) -> anyhow::Result<String> {
    let v: Value = serde_json::from_str(s)?;
    // pretty-print with stable formatting
    Ok(serde_json::to_string_pretty(&v)?)
}

#[cfg(test)]
mod tests {
    use super::{extract_json_blocks, normalize_json, read_markdown};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_file_path(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}_{}.md", prefix, std::process::id(), nanos))
    }

    #[test]
    fn read_markdown_normalizes_crlf() {
        let p = temp_file_path("kb_cli_read_markdown");
        fs::write(&p, "a\r\n\r\nb\r\n").unwrap();

        let got = read_markdown(p.to_str().unwrap()).unwrap();
        assert_eq!(got, "a\n\nb\n");

        fs::remove_file(p).unwrap();
    }

    #[test]
    fn extract_json_blocks_returns_all_json_blocks_in_order() {
        let section = r#"
text
```json
{"a":1}
```
```bash
echo skip
```
```json
{
  "b": 2
}
```
"#;

        let blocks = extract_json_blocks(section);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], "{\"a\":1}\n");
        assert_eq!(blocks[1], "{\n  \"b\": 2\n}\n");
    }

    #[test]
    fn normalize_json_returns_canonical_pretty_json() {
        let got = normalize_json("{\"z\":2,\"a\":1}").unwrap();
        let expected = "{\n  \"a\": 1,\n  \"z\": 2\n}";
        assert_eq!(got, expected);
    }
}
