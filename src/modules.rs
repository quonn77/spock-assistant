//! Maven module discovery and run-configuration ids. Pure, unit-tested natively.

/// Id of the configuration that runs the whole project from its root.
pub const ROOT_ID: &str = "mvn-test";

/// The IDE accepts ids of `a-z 0-9 . - _` only (max 64, starting with a letter or digit), and one
/// invalid id discards the whole list. A module path is therefore encoded reversibly: `[a-z0-9-]`
/// stay, `/` becomes `.`, any other character becomes `_` plus its two-digit lowercase hex code.
const MAX_ID: usize = 64;

/// Configuration id of one module (`mvn-test.<encoded path>`); `None` when it would be too long.
pub fn module_id(rel: &str) -> Option<String> {
    let mut id = format!("{ROOT_ID}.");
    for c in rel.chars() {
        match c {
            'a'..='z' | '0'..='9' | '-' => id.push(c),
            '/' => id.push('.'),
            c if (c as u32) < 0x100 => id.push_str(&format!("_{:02x}", c as u32)),
            _ => return None,
        }
    }
    (id.len() <= MAX_ID && !rel.is_empty()).then_some(id)
}

/// Module path of one of this plugin's configurations: `Some("")` for the root one, `None` for others.
pub fn module_of(config: &str) -> Option<String> {
    if config == ROOT_ID {
        return Some(String::new());
    }
    let enc = config.strip_prefix(ROOT_ID)?.strip_prefix('.')?;
    let mut out = String::new();
    let mut chars = enc.chars();
    while let Some(c) = chars.next() {
        match c {
            '.' => out.push('/'),
            '_' => {
                let hex: String = chars.by_ref().take(2).collect();
                out.push(u8::from_str_radix(&hex, 16).ok().map(char::from)?);
            }
            c => out.push(c),
        }
    }
    Some(out)
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
    fn ids_are_valid_and_round_trip() {
        assert_eq!(module_of(ROOT_ID).as_deref(), Some(""));
        assert_eq!(module_of("other"), None);
        for rel in ["x/y", "gesa-parent-pom", "My_Module/sub.v2", "core/sub-a"] {
            let id = module_id(rel).unwrap();
            assert!(id.starts_with("mvn-test."));
            assert!(id.chars().all(|c| matches!(c, 'a'..='z' | '0'..='9' | '.' | '-' | '_')), "{id}");
            assert_eq!(module_of(&id).as_deref(), Some(rel));
        }
        assert_eq!(module_id("gesa-parent-pom").unwrap(), "mvn-test.gesa-parent-pom");
        assert_eq!(module_id(&"a/".repeat(40)), None, "too long for the IDE");
        assert_eq!(join("", "a"), "a");
        assert_eq!(join("a", "b"), "a/b");
    }
}
