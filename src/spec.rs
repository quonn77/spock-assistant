//! Finds Spock specifications and feature methods in Groovy source text. Pure, unit-tested natively.
//!
//! A line scan, not a parser: it covers the usual shapes (`class FooSpec extends Specification`,
//! `def "does a thing"() {`, `void 'does a thing'()`) and ignores everything else.

/// A runnable place in a Groovy file.
#[derive(Debug, PartialEq, Eq)]
pub struct Marker {
    /// 0-based line.
    pub line: u32,
    /// Simple name of the enclosing spec class.
    pub class: String,
    /// Feature method name; `None` marks the class itself.
    pub feature: Option<String>,
}

fn ident_after<'a>(line: &'a str, keyword: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(i) = line[from..].find(keyword) {
        let at = from + i;
        from = at + keyword.len();
        let before_ok = at == 0 || !line[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_');
        let rest = &line[from..];
        if !before_ok || !rest.starts_with(char::is_whitespace) {
            continue;
        }
        let rest = rest.trim_start();
        let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).unwrap_or(rest.len());
        if end > 0 {
            return Some(&rest[..end]);
        }
    }
    None
}

/// Spec class declared on this line: `class Name extends Something`.
fn spec_class(line: &str) -> Option<&str> {
    let t = line.trim_start();
    if t.starts_with("//") || t.starts_with('*') {
        return None;
    }
    let name = ident_after(line, "class")?;
    let after = line.split_once(name)?.1;
    after.trim_start().starts_with("extends").then_some(name)
}

/// Feature method declared on this line: `def "name"(`, `void 'name'(`, with modifiers allowed.
fn feature_name(line: &str) -> Option<String> {
    let t = line.trim_start();
    if t.starts_with("//") || t.starts_with('*') {
        return None;
    }
    let after = ["def", "void"].iter().find_map(|kw| {
        let at = t.find(kw)?;
        let ok_before = t[..at].split_whitespace().all(|m| matches!(m, "public" | "protected" | "private" | "static" | "final"));
        let rest = &t[at + kw.len()..];
        (ok_before && rest.starts_with(char::is_whitespace)).then(|| rest.trim_start())
    })?;
    let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let body = &after[1..];
    let mut end = None;
    let mut escaped = false;
    for (i, c) in body.char_indices() {
        if escaped {
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == quote {
            end = Some(i);
            break;
        }
    }
    let end = end?;
    // A feature is a method: the closing quote is followed by a parameter list.
    body[end + 1..].trim_start().starts_with('(').then(|| body[..end].replace("\\\"", "\"").replace("\\'", "'"))
}

/// Class and feature markers of a Groovy file, in order. A feature belongs to the last spec class above it.
pub fn find_markers(text: &str) -> Vec<Marker> {
    let mut out = Vec::new();
    let mut class: Option<String> = None;
    for (n, line) in text.lines().enumerate() {
        let line_no = n as u32;
        if let Some(c) = spec_class(line) {
            class = Some(c.to_owned());
            out.push(Marker { line: line_no, class: c.to_owned(), feature: None });
        } else if let (Some(c), Some(f)) = (&class, feature_name(line)) {
            out.push(Marker { line: line_no, class: c.clone(), feature: Some(f) });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r#"package org.acme

import spock.lang.Specification

// class Commented extends Specification
class FooSpec extends Specification {
    def setup() { }

    def "adds two numbers"() {
        expect: 1 + 1 == 2
    }

    private void 'handles a "quoted" word'() {
        expect: true
    }

    def "unrolled #a"(int a) {
        where: a << [1, 2]
    }

    def helper(String s) { s }
    def notAFeature = "text"
}

class Plain { def "not in a spec"() {} }
"#;

    #[test]
    fn finds_the_class_and_its_features() {
        let m = find_markers(SPEC);
        let shown: Vec<_> = m.iter().map(|x| (x.line, x.class.as_str(), x.feature.as_deref())).collect();
        assert_eq!(
            shown,
            [
                (5, "FooSpec", None),
                (8, "FooSpec", Some("adds two numbers")),
                (12, "FooSpec", Some("handles a \"quoted\" word")),
                (16, "FooSpec", Some("unrolled #a")),
            ]
        );
    }

    #[test]
    fn a_class_without_extends_is_not_a_spec_and_its_methods_belong_to_nothing_new() {
        let m = find_markers("class Plain {\n def \"x\"() {}\n}\n");
        assert!(m.is_empty());
    }
}
