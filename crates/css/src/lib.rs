//! Hand-written CSS parsing.
//!
//! Parses a CSS string into a [`Stylesheet`] of [`Rule`]s. Each rule is a list of raw
//! selector strings (interpreted by the `style` crate) plus a list of `(property, value)`
//! declarations. Comments are stripped, at-rules (`@media`, `@font-face`, …) are consumed
//! gracefully (including any balanced `{ … }` block) without emitting bogus rules, and
//! malformed input never panics.

/// A parsed CSS stylesheet: an ordered list of rules.
#[derive(Debug, Clone, Default)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

/// A single rule: a group of selectors and the declarations they apply.
#[derive(Debug, Clone, Default)]
pub struct Rule {
    pub selectors: Vec<String>,
    pub declarations: Vec<(String, String)>,
}

/// Parse a CSS string into a [`Stylesheet`].
pub fn parse(css: &str) -> Stylesheet {
    let stripped = strip_comments(css);
    let bytes: Vec<char> = stripped.chars().collect();
    let mut pos = 0usize;
    let mut rules = Vec::new();

    while pos < bytes.len() {
        // Skip leading whitespace.
        while pos < bytes.len() && bytes[pos].is_whitespace() {
            pos += 1;
        }
        if pos >= bytes.len() {
            break;
        }

        if bytes[pos] == '@' {
            // At-rule: consume up to a `;` (no block) or a balanced `{ … }` block.
            pos = consume_at_rule(&bytes, pos);
            continue;
        }

        // Read the selector prelude up to the next `{` (or end / stray `}`).
        let prelude_start = pos;
        while pos < bytes.len() && bytes[pos] != '{' && bytes[pos] != '}' {
            pos += 1;
        }

        if pos >= bytes.len() || bytes[pos] == '}' {
            // No declaration block — skip a stray `}` and ignore the dangling prelude.
            if pos < bytes.len() && bytes[pos] == '}' {
                pos += 1;
            }
            continue;
        }

        let prelude: String = bytes[prelude_start..pos].iter().collect();
        // Skip the `{`.
        pos += 1;

        // Read the declaration block body up to the matching `}`.
        let body_start = pos;
        while pos < bytes.len() && bytes[pos] != '}' {
            pos += 1;
        }
        let body: String = bytes[body_start..pos].iter().collect();
        // Skip the closing `}` if present.
        if pos < bytes.len() {
            pos += 1;
        }

        let selectors = parse_selector_list(&prelude);
        let declarations = parse_declarations(&body);
        if !selectors.is_empty() {
            rules.push(Rule { selectors, declarations });
        }
    }

    Stylesheet { rules }
}

/// Parse an inline `style="..."` attribute value (or any bare declaration block) into a
/// list of `(property, value)` pairs. Property names are lowercased; values are trimmed
/// and kept as-is.
pub fn parse_declarations(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for chunk in s.split(';') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        if let Some(idx) = chunk.find(':') {
            let prop = chunk[..idx].trim().to_ascii_lowercase();
            let val = chunk[idx + 1..].trim().to_string();
            if !prop.is_empty() && !val.is_empty() {
                out.push((prop, val));
            }
        }
    }
    out
}

/// Split a comma-separated selector list into trimmed, non-empty raw selector strings.
fn parse_selector_list(s: &str) -> Vec<String> {
    s.split(',')
        .map(|sel| sel.trim().to_string())
        .filter(|sel| !sel.is_empty())
        .collect()
}

/// Strip `/* ... */` comments. Unterminated comments swallow the rest of the input.
fn strip_comments(css: &str) -> String {
    let chars: Vec<char> = css.chars().collect();
    let mut out = String::with_capacity(css.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            // Find the terminating `*/`.
            i += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            // Skip the closing `*/` if present.
            i = (i + 2).min(chars.len());
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// Consume an at-rule starting at `start` (where `bytes[start] == '@'`). If the at-rule has
/// a `{ … }` block, consume the balanced block; otherwise consume up to and including the
/// terminating `;`. Returns the position just past the at-rule.
fn consume_at_rule(bytes: &[char], start: usize) -> usize {
    let mut pos = start;
    // Scan the prelude looking for the first `{`, `;`, or end.
    while pos < bytes.len() && bytes[pos] != '{' && bytes[pos] != ';' {
        pos += 1;
    }
    if pos >= bytes.len() {
        return pos;
    }
    if bytes[pos] == ';' {
        return pos + 1;
    }
    // bytes[pos] == '{': consume a balanced block.
    let mut depth = 0usize;
    while pos < bytes.len() {
        match bytes[pos] {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return pos + 1;
                }
            }
            _ => {}
        }
        pos += 1;
    }
    pos
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_parse_has_no_rules() {
        let sheet = parse("");
        assert_eq!(sheet.rules.len(), 0);
    }

    #[test]
    fn parses_multiple_rules() {
        let sheet = parse("h1 { color: red; font-size: 32px } p { color: #00f }");
        assert_eq!(sheet.rules.len(), 2);
        assert_eq!(sheet.rules[0].selectors, vec!["h1"]);
        assert_eq!(
            sheet.rules[0].declarations,
            vec![
                ("color".to_string(), "red".to_string()),
                ("font-size".to_string(), "32px".to_string())
            ]
        );
        assert_eq!(sheet.rules[1].selectors, vec!["p"]);
        assert_eq!(
            sheet.rules[1].declarations,
            vec![("color".to_string(), "#00f".to_string())]
        );
    }

    #[test]
    fn parses_grouped_selectors() {
        let sheet = parse("h1, h2 , .note { color: blue; }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].selectors, vec!["h1", "h2", ".note"]);
    }

    #[test]
    fn strips_comments() {
        let sheet = parse("/* a comment */ p /* mid */ { color: /* x */ red; } /* trailing */");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].selectors, vec!["p"]);
        assert_eq!(
            sheet.rules[0].declarations,
            vec![("color".to_string(), "red".to_string())]
        );
    }

    #[test]
    fn skips_media_at_rule() {
        let sheet = parse(
            "p { color: red } @media (max-width: 600px) { p { color: green } } h1 { color: blue }",
        );
        // The @media block (and its nested rules) is skipped; only the two top-level rules remain.
        assert_eq!(sheet.rules.len(), 2);
        assert_eq!(sheet.rules[0].selectors, vec!["p"]);
        assert_eq!(sheet.rules[1].selectors, vec!["h1"]);
    }

    #[test]
    fn skips_at_rule_with_semicolon() {
        let sheet = parse("@import url(\"x.css\"); p { color: red }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].selectors, vec!["p"]);
    }

    #[test]
    fn parses_inline_declarations() {
        let decls = parse_declarations("color: red; font-size: 20px ; font-weight:bold");
        assert_eq!(
            decls,
            vec![
                ("color".to_string(), "red".to_string()),
                ("font-size".to_string(), "20px".to_string()),
                ("font-weight".to_string(), "bold".to_string()),
            ]
        );
    }

    #[test]
    fn malformed_input_does_not_panic() {
        let _ = parse("p { color: red");
        let _ = parse("}}} @media {{{ ");
        let _ = parse("/* unterminated");
        let _ = parse("{ no selector }");
        let _ = parse("@font-face");
    }
}
