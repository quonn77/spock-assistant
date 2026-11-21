//! Maven module discovery and run-configuration ids. Pure, unit-tested natively.

/// Id of the configuration that runs the whole project from its root.
pub const ROOT_ID: &str = "mvn-test";

/// Configuration id of one module: `mvn-test:<module path relative to the project root>`.
pub fn module_id(rel: &str) -> String {
    format!("{ROOT_ID}:{rel}")
}

/// Module path of one of this plugin's configurations: `Some("")` for the root one, `None` for others.
pub fn module_of(config: &str) -> Option<&str> {
    if config == ROOT_ID {
        return Some("");
    }
    config.strip_prefix(ROOT_ID).and_then(|r| r.strip_prefix(':'))
}

/// `<module>` entries of a pom's `<modules>` block (comments ignored), in order.
pub fn parse_modules(pom: &str) -> Vec<String> {
    let mut text = String::with_capacity(pom.len());
    let mut rest = pom;
    while let Some(i) = rest.find("<!--") {
        text.push_str(&rest[..i]);
        rest = match rest[i..].find("-->") {
            Some(e) => &rest[i + e + 3..],
            None => "",
        };
    }
    text.push_str(rest);
    let Some(start) = text.find("<modules>") else { return vec![] };
    let after = &text[start + "<modules>".len()..];
    let block = after.split("</modules>").next().unwrap_or(after);
    let mut out = Vec::new();
    let mut cur = block;
    while let Some(i) = cur.find("<module>") {
        let body = &cur[i + "<module>".len()..];
        let Some(end) = body.find("</module>") else { break };
        let name = body[..end].trim().trim_matches('/').replace('\\', "/");
        if !name.is_empty() && !name.starts_with("..") && !out.contains(&name) {
            out.push(name);
        }
        cur = &body[end + "</module>".len()..];
    }
    out
}

/// Joins a module path under a parent module path (both relative, `/`-separated).
pub fn join(parent: &str, child: &str) -> String {
    if parent.is_empty() { child.to_owned() } else { format!("{parent}/{child}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_modules_and_ignores_comments() {
        let pom = r#"<project><modules>
            <module>a</module>
            <!-- <module>old</module> -->
            <module> b/c </module>
            <module>../outside</module>
            <module>a</module>
        </modules><build><module>not-this</module></build></project>"#;
        assert_eq!(parse_modules(pom), ["a", "b/c"]);
        assert!(parse_modules("<project/>").is_empty());
    }

    #[test]
    fn ids_round_trip() {
        assert_eq!(module_of(ROOT_ID), Some(""));
        assert_eq!(module_of(&module_id("x/y")), Some("x/y"));
        assert_eq!(module_of("other"), None);
        assert_eq!(join("", "a"), "a");
        assert_eq!(join("a", "b"), "a/b");
    }
}
