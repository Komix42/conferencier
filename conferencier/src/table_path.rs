//! Diagnostic paths for manual [`ConfigScope`](crate::ConfigScope) and
//! [`ConfigTable`](crate::ConfigTable) implementations.
//!
//! These helpers build the same path strings as the derived code. Each dotted
//! segment follows TOML key notation: it is written bare when it is non-empty and
//! consists only of `A-Z a-z 0-9 _ -`, and quoted otherwise. Quoted segments use
//! a canonical TOML basic string: `\"` and `\\`; `\b`, `\t`, `\n`, `\f` and `\r`
//! for the named control characters; uppercase `\uXXXX` for the other characters
//! in U+0000–U+001F and U+007F–U+009F; every other character literally. An
//! element of an array of tables appends a zero-based `[index]`.
//!
//! The results are diagnostic strings only. They are not store addresses, they are
//! not TOML dotted keys once they contain an index, and the helpers neither access
//! a store nor check that a key or array exists. Pass raw section and key names;
//! the `parent` and `path` arguments are already formatted paths. A path received
//! by [`ConfigTable::from_table`](crate::ConfigTable::from_table) or
//! [`ConfigTable::to_table`](crate::ConfigTable::to_table) is already formatted and
//! can be extended directly; do not pass it through [`section_path`] again.
//!
//! ```
//! use conferencier::table_path::{child_path, element_path, section_path};
//!
//! let root = section_path("App.prod");
//! assert_eq!(root, "\"App.prod\"");
//! assert_eq!(section_path("App"), "App");
//! assert_eq!(section_path(""), "\"\"");
//! assert_eq!(child_path("", "App.prod"), root);
//!
//! let server = child_path(&root, "server.prod");
//! assert_eq!(server, "\"App.prod\".\"server.prod\"");
//! let upstream = element_path(&child_path(&root, "upstream"), 1);
//! assert_eq!(upstream, "\"App.prod\".upstream[1]");
//! ```

/// Formats one raw top-level section name as a diagnostic path.
///
/// Names that are not bare TOML keys, including names containing a dot and the
/// empty name, are quoted.
pub fn section_path(section: &str) -> String {
    let mut path = String::new();
    push_segment(&mut path, section);
    path
}

/// Appends one raw `key` to the already formatted `parent` path.
///
/// An empty `parent` denotes the document root, so `child_path("", key)` equals
/// [`section_path(key)`](section_path) and has no leading dot.
pub fn child_path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        return section_path(key);
    }
    let mut path = format!("{parent}.");
    push_segment(&mut path, key);
    path
}

/// Appends the zero-based `index` of an array-of-tables element to the already
/// formatted array `path`.
pub fn element_path(path: &str, index: usize) -> String {
    format!("{path}[{index}]")
}

fn push_segment(path: &mut String, key: &str) {
    if !key.is_empty()
        && key
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        path.push_str(key);
        return;
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
            '\0'..='\u{1f}' | '\u{7f}'..='\u{9f}' => {
                path.push_str(&format!("\\u{:04X}", u32::from(ch)));
            }
            other => path.push(other),
        }
    }
    path.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_quote_only_non_bare_segments() {
        for key in ["server", "a-Z_09", "123"] {
            assert_eq!(section_path(key), key);
            assert_eq!(child_path("App", key), format!("App.{key}"));
        }
        for key in [
            "",
            "my key",
            "a.b",
            "x[1]",
            "\u{e9}",
            "\"\\\n\t\r\u{8}\u{c}\0\u{1b}\u{7f}\u{80}\u{9f}\u{a0}",
        ] {
            let quoted = section_path(key);
            assert!(quoted.starts_with('"') && quoted.ends_with('"'));
            let parsed: toml::Table = format!("{quoted} = 1").parse().unwrap();
            assert_eq!(parsed.keys().next().unwrap(), key);
            assert_eq!(child_path("App", key), format!("App.{quoted}"));
            assert_eq!(child_path("", key), quoted);
        }
        assert_eq!(
            child_path("App.with.dot", "my key"),
            "App.with.dot.\"my key\""
        );
        assert_eq!(
            element_path(&child_path("App", "upstream"), 1),
            "App.upstream[1]"
        );
        assert_eq!(
            child_path("App.upstream[1]", "port"),
            "App.upstream[1].port"
        );
    }

    #[test]
    fn quoted_segments_use_canonical_escapes() {
        assert_eq!(section_path(""), "\"\"");
        assert_eq!(section_path("a\"b\\c"), r#""a\"b\\c""#);
        assert_eq!(section_path("\u{8}\t\n\u{c}\r"), r#""\b\t\n\f\r""#);
        assert_eq!(
            section_path("\0\u{1}\u{1b}\u{1f}\u{7f}\u{80}\u{9f}"),
            r#""\u0000\u0001\u001B\u001F\u007F\u0080\u009F""#
        );
        assert_eq!(
            section_path("caf\u{e9} \u{a0}\u{1f600}"),
            "\"caf\u{e9} \u{a0}\u{1f600}\""
        );
    }
}
