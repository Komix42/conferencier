//! Diagnostic paths of nested tables (RFC-0004 E2–E3).

/// Appends `key` to `parent`, quoting it as a TOML basic string unless it is a
/// non-empty bare key.
pub fn child_path(parent: &str, key: &str) -> String {
    let mut path = format!("{parent}.");
    if !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        path.push_str(key);
        return path;
    }

    path.push('"');
    for ch in key.chars() {
        match ch {
            '"' => path.push_str("\\\""),
            '\\' => path.push_str("\\\\"),
            '\u{8}' => path.push_str("\\b"),
            '\t' => path.push_str("\\t"),
            '\n' => path.push_str("\\n"),
            '\u{c}' => path.push_str("\\f"),
            '\r' => path.push_str("\\r"),
            '\0'..='\u{1f}' | '\u{7f}' => path.push_str(&format!("\\u{:04X}", u32::from(ch))),
            other => path.push(other),
        }
    }
    path.push('"');
    path
}

/// Returns the path of the zero-based `index` element of an array of tables.
pub fn element_path(path: &str, index: usize) -> String {
    format!("{path}[{index}]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_quote_only_nested_segments() {
        for key in ["server", "a-Z_09", "123"] {
            assert_eq!(child_path("App", key), format!("App.{key}"));
        }
        for key in [
            "",
            "my key",
            "a.b",
            "x[1]",
            "\u{e9}",
            "\"\\\n\t\r\u{8}\u{c}\0\u{1b}\u{7f}",
        ] {
            let path = child_path("App", key);
            let quoted = path.strip_prefix("App.").unwrap();
            let parsed: toml::Table = format!("{quoted} = 1").parse().unwrap();
            assert_eq!(parsed.keys().next().unwrap(), key);
            assert!(quoted.starts_with('"') && quoted.ends_with('"'));
        }
        assert_eq!(
            child_path("App.with.dot", "my key"),
            "App.with.dot.\"my key\""
        );
        assert_eq!(child_path("App", "\"\\\n"), "App.\"\\\"\\\\\\n\"");
        assert_eq!(
            element_path(&child_path("App", "upstream"), 1),
            "App.upstream[1]"
        );
        assert_eq!(
            child_path("App.upstream[1]", "port"),
            "App.upstream[1].port"
        );
    }
}
