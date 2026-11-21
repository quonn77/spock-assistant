//! Pure parsing of Maven Surefire output: no host calls, so it is unit-tested natively.
//!
//! Surefire's console shows one line per test class, not per feature method, so the per-feature
//! results come from the XML report Surefire writes for the class (`TEST-<class>.xml`).

/// What one console line means for the test tree.
#[derive(Debug, PartialEq, Eq)]
pub enum Line {
    /// `Running <fqcn>`: a test class starts.
    Running(String),
    /// `Tests run: ..., -- in <fqcn>`: a test class is finished (its XML report is complete).
    ClassDone { fqcn: String, failed: u32, skipped: u32, run: u32 },
    Other,
}

/// Outcome of one `<testcase>`.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Passed,
    Failed(String),
    Skipped(Option<String>),
}

/// One `<testcase>` of a Surefire XML report.
#[derive(Debug, PartialEq, Eq)]
pub struct Case {
    pub name: String,
    pub time_ms: Option<u32>,
    pub outcome: Outcome,
}

/// Removes ANSI colour sequences and the `[INFO] ` / `[ERROR] ` / `[WARNING] ` prefix.
fn clean(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    let t = out.trim();
    for p in ["[INFO]", "[ERROR]", "[WARNING]"] {
        if let Some(rest) = t.strip_prefix(p) {
            return rest.trim().to_owned();
        }
    }
    t.to_owned()
}

fn number_after(s: &str, key: &str) -> u32 {
    s.split_once(key)
        .map(|(_, r)| r.trim_start().chars().take_while(char::is_ascii_digit).collect::<String>())
        .and_then(|d| d.parse().ok())
        .unwrap_or(0)
}

/// Classifies one console line.
pub fn classify(raw: &str) -> Line {
    let line = clean(raw);
    if let Some(name) = line.strip_prefix("Running ") {
        let name = name.trim();
        if !name.is_empty() && !name.contains(' ') {
            return Line::Running(name.to_owned());
        }
    }
    if line.starts_with("Tests run:") {
        if let Some((counts, fqcn)) = line.split_once(" -- in ") {
            let fqcn = fqcn.trim();
            if !fqcn.is_empty() {
                return Line::ClassDone {
                    fqcn: fqcn.to_owned(),
                    run: number_after(counts, "Tests run:"),
                    failed: number_after(counts, "Failures:") + number_after(counts, "Errors:"),
                    skipped: number_after(counts, "Skipped:"),
                };
            }
        }
    }
    Line::Other
}

pub fn simple_name(fqcn: &str) -> &str {
    fqcn.rsplit('.').next().unwrap_or(fqcn)
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|e| *e <= 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..end];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => ent
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Value of attribute `name` inside an opening tag (`<tag a="x" b='y'>`).
fn attr(tag: &str, name: &str) -> Option<String> {
    let mut from = 0;
    while let Some(i) = tag[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let before_ok = tag[..at].chars().next_back().is_some_and(char::is_whitespace);
        let rest = tag[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let rest = rest.trim_start();
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        if !before_ok {
            continue;
        }
        let body = &rest[1..];
        let end = body.find(quote)?;
        return Some(unescape(&body[..end]));
    }
    None
}

/// End of the opening tag starting at `s` (index of its `>`), skipping `>` inside quotes.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

fn first_line(s: &str) -> String {
    s.trim().lines().next().unwrap_or("").trim().to_owned()
}

fn cap(mut s: String) -> String {
    const MAX: usize = 4000;
    if s.len() > MAX {
        let mut cut = MAX;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
        s.push('…');
    }
    s
}

fn failure_message(body: &str) -> Option<String> {
    for kind in ["<failure", "<error"] {
        let Some(at) = body.find(kind) else { continue };
        let after = &body[at..];
        let Some(end) = tag_end(after) else { continue };
        let tag = &after[..end];
        let text = after[end + 1..].split("</").next().map(unescape).unwrap_or_default();
        let message = attr(tag, "message").filter(|m| !m.trim().is_empty());
        let ty = attr(tag, "type");
        let message = match (message, ty) {
            (Some(m), _) => m,
            (None, Some(t)) if !first_line(&text).is_empty() => format!("{t}: {}", first_line(&text)),
            (None, Some(t)) => t,
            (None, None) => first_line(&text),
        };
        return Some(cap(if message.is_empty() { "Test failed".to_owned() } else { message }));
    }
    None
}

/// Every `<testcase>` of a Surefire XML report, in file order.
pub fn parse_cases(xml: &str) -> Vec<Case> {
    let mut cases = Vec::new();
    let mut rest = xml;
    while let Some(at) = rest.find("<testcase") {
        let after = &rest[at..];
        let is_tag = after["<testcase".len()..].chars().next().is_some_and(char::is_whitespace);
        let Some(end) = tag_end(after).filter(|_| is_tag) else {
            rest = &rest[at + "<testcase".len()..];
            continue;
        };
        let tag = &after[..end];
        let self_closing = tag.ends_with('/');
        let (body, consumed) = if self_closing {
            ("", end + 1)
        } else {
            match after[end + 1..].find("</testcase>") {
                Some(close) => (&after[end + 1..end + 1 + close], end + 1 + close + "</testcase>".len()),
                None => (&after[end + 1..], after.len()),
            }
        };
        rest = &after[consumed..];
        let Some(name) = attr(tag, "name") else { continue };
        let time_ms = attr(tag, "time")
            .and_then(|t| t.replace(',', ".").parse::<f64>().ok())
            .map(|s| (s * 1000.0).round().clamp(0.0, u32::MAX as f64) as u32);
        let outcome = if let Some(m) = failure_message(body) {
            Outcome::Failed(m)
        } else if let Some(at) = body.find("<skipped") {
            let tag = &body[at..];
            let msg = tag_end(tag).and_then(|e| attr(&tag[..e], "message"));
            Outcome::Skipped(msg)
        } else {
            Outcome::Passed
        };
        cases.push(Case { name, time_ms, outcome });
    }
    cases
}

/// Surefire `-Dtest` pattern for a feature name: every character that is not a word character
/// becomes `?`, because Spock feature names are free text and `-Dtest=Class#method` is a pattern.
fn method_pattern(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == '_' { c } else { '?' }).collect()
}

/// Full Maven argument list that runs only the selected tests. A path is `[class]` or
/// `[class, feature]` (the names shown in the test tree); anything else is not selectable.
pub fn select_args(tests: &[Vec<String>]) -> Option<Vec<String>> {
    if tests.is_empty() {
        return None;
    }
    let mut filters: Vec<String> = Vec::new();
    for path in tests {
        let f = match path.as_slice() {
            [class] => class.clone(),
            [class, feature] => format!("{class}#{}", method_pattern(feature)),
            _ => return None,
        };
        if !filters.contains(&f) {
            filters.push(f);
        }
    }
    Some(vec![
        "-B".into(),
        "test".into(),
        format!("-Dtest={}", filters.join(",")),
        "-Dsurefire.failIfNoSpecifiedTests=false".into(),
        "-DfailIfNoTests=false".into(),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_surefire_console_lines() {
        assert_eq!(classify("[INFO] Running org.acme.FooSpec"), Line::Running("org.acme.FooSpec".into()));
        assert_eq!(classify("\u{1b}[0m[INFO] Running org.acme.FooSpec"), Line::Running("org.acme.FooSpec".into()));
        assert_eq!(
            classify("[ERROR] Tests run: 3, Failures: 1, Errors: 1, Skipped: 1, Time elapsed: 0.5 s <<< FAILURE! -- in org.acme.FooSpec"),
            Line::ClassDone { fqcn: "org.acme.FooSpec".into(), run: 3, failed: 2, skipped: 1 }
        );
        assert_eq!(classify("[INFO] Tests run: 3, Failures: 0, Errors: 0, Skipped: 0"), Line::Other);
        assert_eq!(classify("[INFO] Running a very noisy sentence"), Line::Other);
    }

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="org.acme.FooSpec" tests="5">
  <properties><property name="a" value="b"/></properties>
  <testcase name="adds two numbers" classname="FooSpec" time="0.766"/>
  <testcase name="a &quot;quoted&quot; name &amp; more" classname="FooSpec" time="0.01">
    <system-out><![CDATA[noise <testcase name="fake"/>]]></system-out>
  </testcase>
  <testcase name="fails with a condition" classname="FooSpec" time="1,5">
    <failure message="Condition not satisfied:&#10;&#10;a == b&#10;| |  |&#10;1 2  3" type="org.spockframework.runtime.ConditionNotSatisfiedError">org.spockframework.runtime.ConditionNotSatisfiedError: Condition not satisfied
	at FooSpec.fails(FooSpec.groovy:12)</failure>
  </testcase>
  <testcase name="blows up" classname="FooSpec" time="0">
    <error type="java.lang.IllegalStateException">java.lang.IllegalStateException: boom
	at X</error>
  </testcase>
  <testcase name="is ignored" classname="FooSpec" time="0"><skipped message="Not yet"/></testcase>
</testsuite>"#;

    #[test]
    fn reads_every_testcase_with_its_outcome() {
        let cases = parse_cases(XML);
        let names: Vec<_> = cases.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            ["adds two numbers", "a \"quoted\" name & more", "fails with a condition", "blows up", "is ignored"]
        );
        assert_eq!(cases[0], Case { name: "adds two numbers".into(), time_ms: Some(766), outcome: Outcome::Passed });
        assert_eq!(cases[1].outcome, Outcome::Passed, "CDATA text is not a testcase");
        assert_eq!(cases[2].time_ms, Some(1500));
        assert_eq!(
            cases[2].outcome,
            Outcome::Failed("Condition not satisfied:\n\na == b\n| |  |\n1 2  3".into())
        );
        assert_eq!(
            cases[3].outcome,
            Outcome::Failed("java.lang.IllegalStateException: java.lang.IllegalStateException: boom".into())
        );
        assert_eq!(cases[4].outcome, Outcome::Skipped(Some("Not yet".into())));
    }

    #[test]
    fn classname_attribute_is_not_mistaken_for_name() {
        let cases = parse_cases(r#"<testcase classname="Only" name="real" time="1"/>"#);
        assert_eq!(cases[0].name, "real");
    }

    #[test]
    fn selection_builds_a_full_maven_command() {
        let args = select_args(&[
            vec!["FooSpec".into(), "adds two numbers".into()],
            vec!["BarSpec".into()],
            vec!["FooSpec".into(), "adds two numbers".into()],
        ])
        .unwrap();
        assert_eq!(args[..3], ["-B", "test", "-Dtest=FooSpec#adds?two?numbers,BarSpec"]);
        assert_eq!(select_args(&[]), None);
        assert_eq!(select_args(&[vec!["a".into(), "b".into(), "c".into()]]), None);
    }
}
