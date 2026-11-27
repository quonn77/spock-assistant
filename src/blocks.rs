//! Checks the order of Spock blocks in feature methods. Pure, unit-tested natively.
//!
//! A line scan with brace tracking, not a parser: labels count only at the top level of a method
//! body inside a class that extends something. The allowed successions follow the Spock reference
//! documentation ("Blocks"): `given`/`setup` first, `when` always followed by `then`, `cleanup`
//! only before `where`, `where` last (only `filter` after it); `and` continues the block before it.

/// One block problem: 0-based line, UTF-16 columns of the label, message.
#[derive(Debug, PartialEq, Eq)]
pub struct Problem {
    pub line: u32,
    pub start: u32,
    pub end: u32,
    pub message: String,
}

const LABELS: [&str; 9] = ["given", "setup", "expect", "when", "then", "and", "cleanup", "where", "filter"];

/// Brace depth at the start of every line (ignoring braces in strings and comments), and whether
/// the line starts in code rather than inside a multi-line string or comment.
fn depths(text: &str) -> Vec<(u32, bool)> {
    #[derive(Clone, Copy, PartialEq)]
    enum S {
        Code,
        Block,
        Str(char),
        Triple(char),
    }
    let mut out = Vec::new();
    let mut depth: i64 = 0;
    let mut state = S::Code;
    for line in text.lines() {
        out.push((depth.max(0) as u32, state == S::Code));
        let b: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < b.len() {
            let c = b[i];
            let next = b.get(i + 1).copied();
            match state {
                S::Code => match c {
                    '/' if next == Some('/') => break,
                    '/' if next == Some('*') => {
                        state = S::Block;
                        i += 1;
                    }
                    '"' | '\'' if next == Some(c) && b.get(i + 2) == Some(&c) => {
                        state = S::Triple(c);
                        i += 2;
                    }
                    '"' | '\'' => state = S::Str(c),
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                },
                S::Block => {
                    if c == '*' && next == Some('/') {
                        state = S::Code;
                        i += 1;
                    }
                }
                S::Str(q) => {
                    if c == '\\' {
                        i += 1;
                    } else if c == q {
                        state = S::Code;
                    }
                }
                S::Triple(q) => {
                    if c == '\\' {
                        i += 1;
                    } else if c == q && next == Some(q) && b.get(i + 2) == Some(&q) {
                        state = S::Code;
                        i += 2;
                    }
                }
            }
            i += 1;
        }
        // A single-quoted or double-quoted string does not span lines.
        if matches!(state, S::Str(_)) {
            state = S::Code;
        }
    }
    out
}

/// Block label at the start of this line, with its UTF-16 columns.
fn label(line: &str) -> Option<(&'static str, u32, u32)> {
    let lead = line.len() - line.trim_start().len();
    let t = &line[lead..];
    let name = LABELS.iter().find(|l| t.starts_with(**l))?;
    let rest = &t[name.len()..];
    let colon = rest.trim_start();
    if !colon.starts_with(':') || colon.starts_with("::") {
        return None;
    }
    let col = line[..lead].encode_utf16().count() as u32;
    Some((name, col, col + name.len() as u32))
}

/// A method declared on this line: modifiers, `def`/`void`/a type, a quoted or plain name, `(`.
fn is_method_header(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with("//") || t.starts_with('*') {
        return false;
    }
    let mut rest = t;
    loop {
        let word: &str = rest.split(|c: char| c.is_whitespace()).next().unwrap_or("");
        if matches!(word, "public" | "protected" | "private" | "static" | "final" | "def" | "void") && !word.is_empty() {
            rest = rest[word.len()..].trim_start();
            if matches!(word, "def" | "void") {
                break;
            }
        } else {
            return false;
        }
    }
    let after = match rest.chars().next() {
        Some(q @ ('"' | '\'')) => match rest[1..].find(q) {
            Some(end) => &rest[end + 2..],
            None => return false,
        },
        Some(c) if c.is_alphabetic() || c == '_' => rest.trim_start_matches(|c: char| c.is_alphanumeric() || c == '_'),
        _ => return false,
    };
    after.trim_start().starts_with('(')
}

fn first(name: &str) -> bool {
    matches!(name, "given" | "setup")
}

/// Whether `next` may follow `prev` (both real blocks, never `and`).
fn allowed(prev: &str, next: &str) -> bool {
    match prev {
        "given" | "setup" => matches!(next, "expect" | "when" | "cleanup" | "where"),
        "expect" => matches!(next, "when" | "cleanup" | "where"),
        "when" => next == "then",
        "then" => matches!(next, "expect" | "when" | "then" | "cleanup" | "where"),
        "cleanup" => next == "where",
        "where" => next == "filter",
        _ => false,
    }
}

fn message(prev: &str, next: &str) -> String {
    if next == "then" {
        format!("'then' must follow a 'when' block (found after '{prev}')")
    } else if first(next) {
        format!("'{next}' must be the first block")
    } else if prev == "when" {
        format!("'when' must be followed by 'then' (found '{next}')")
    } else if prev == "where" || prev == "filter" {
        format!("'{prev}' must be the last block (found '{next}' after it)")
    } else {
        format!("'{next}' cannot follow '{prev}'")
    }
}

/// Block order problems of the Spock features in a Groovy file.
pub fn check(text: &str) -> Vec<Problem> {
    let lines: Vec<&str> = text.lines().collect();
    let depth = depths(text);
    let mut out = Vec::new();
    let mut in_spec = false;
    let mut n = 0;
    while n < lines.len() {
        let line = lines[n];
        if crate::spec::is_spec_class(line) {
            in_spec = true;
        }
        if !(in_spec && is_method_header(line)) {
            n += 1;
            continue;
        }
        let d = depth[n].0;
        let mut prev: Option<(&str, u32, u32, u32)> = None; // last real block and its label position
        let mut m = n + 1;
        while m < lines.len() && depth[m].0 > d {
            if depth[m] == (d + 1, true)
                && let Some((name, s, e)) = label(lines[m])
                && name != "and"
            {
                let problem = match prev {
                    None if name == "then" => Some("'then' must follow a 'when' block (it is the first block)".into()),
                    Some((p, ..)) if !allowed(p, name) => Some(message(p, name)),
                    _ => None,
                };
                if let Some(message) = problem {
                    out.push(Problem { line: m as u32, start: s, end: e, message });
                }
                prev = Some((name, m as u32, s, e));
            }
            m += 1;
        }
        if let Some(("when", l, s, e)) = prev {
            out.push(Problem { line: l, start: s, end: e, message: "'when' must be followed by 'then'".into() });
        }
        n = m.max(n + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<(u32, String)> {
        check(text).into_iter().map(|p| (p.line, p.message)).collect()
    }

    fn feature(body: &str) -> String {
        format!("class FooSpec extends Specification {{\n    def \"f\"() {{\n{body}\n    }}\n}}\n")
    }

    #[test]
    fn a_well_formed_feature_has_no_problems() {
        let body = "        given: \"a list\"\n        def l = []\n        and:\n        l << 1\n        when:\n        l << 2\n        then:\n        l.size() == 2\n        when: l.clear()\n        then: l.empty\n        cleanup:\n        l = null\n        where:\n        a | b\n        1 | 2";
        assert_eq!(found(&feature(body)), []);
        assert_eq!(found(&feature("        expect: 1 + 1 == 2\n        where: x << [1]")), []);
        assert_eq!(found(&feature("        expect: true\n        when: x()\n        then: true")), []);
    }

    #[test]
    fn reports_misplaced_blocks_on_their_label() {
        let p = check(&feature("        expect: true\n        then: true"));
        assert_eq!(p.len(), 1);
        assert_eq!((p[0].line, p[0].start, p[0].end), (3, 8, 12));
        assert!(p[0].message.contains("'then' must follow a 'when'"), "{}", p[0].message);

        assert_eq!(found(&feature("        when: x()\n        expect: true")).len(), 1);
        assert_eq!(found(&feature("        when: x()"))[0].1, "'when' must be followed by 'then'");
        assert!(found(&feature("        expect: true\n        given: 1"))[0].1.contains("must be the first block"));
        assert!(found(&feature("        where: a << [1]\n        expect: a"))[0].1.contains("must be the last block"));
        assert!(found(&feature("        cleanup: x()\n        expect: true"))[0].1.contains("cannot follow 'cleanup'"));
        assert!(found(&feature("        then: true"))[0].1.contains("first block"));
    }

    #[test]
    fn ignores_labels_in_closures_strings_comments_and_plain_classes() {
        let body = "        when:\n        x { then: 1 }\n        with(y) {\n            expect: 1\n        }\n        def s = \"\"\"\n        expect:\n        \"\"\"\n        // expect:\n        then: true";
        assert_eq!(found(&feature(body)), []);
        assert_eq!(found("class Plain {\n    def f() {\n        when: 1\n    }\n}\n"), []);
        assert_eq!(found(&feature("        def m = [when: 1]\n        expect: m")), []);
    }

    #[test]
    fn plain_named_methods_and_two_features_are_checked_separately() {
        let text = "class FooSpec extends Specification {\n    void plain() {\n        when: x()\n    }\n\n    def 'b'() {\n        then: 1\n    }\n}\n";
        assert_eq!(found(text).iter().map(|p| p.0).collect::<Vec<_>>(), [2, 6]);
    }
}
