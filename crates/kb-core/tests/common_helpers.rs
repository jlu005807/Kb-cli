#[path = "../../../tests/common/mod.rs"]
mod common;

#[test]
fn helper_read_markdown_normalizes_newlines() {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    let p = std::env::temp_dir().join(format!(
        "kb_cli_common_read_{}_{}.md",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));

    fs::write(&p, "x\r\ny\r\n").unwrap();
    let got = common::read_markdown(p.to_str().unwrap()).unwrap();
    assert_eq!(got, "x\ny\n");
    fs::remove_file(p).unwrap();
}

#[test]
fn helper_extract_json_blocks_finds_first_two_json_blocks() {
    let section = r#"
```json
{"ok":true}
```

```json
{"n":1}
```
"#;

    let blocks = common::extract_json_blocks(section);
    assert_eq!(blocks, vec!["{\"ok\":true}\n", "{\"n\":1}\n"]);
}

#[test]
fn helper_normalize_json_pretty_and_sorted() {
    let got = common::normalize_json("{\"b\":2,\"a\":1}").unwrap();
    assert_eq!(got, "{\n  \"a\": 1,\n  \"b\": 2\n}");
}
