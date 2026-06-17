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
    /// The raw `@media` query this rule lives under (`None` if not inside any `@media`).
    /// Nested `@media` queries are joined with `" and "`. `@layer`/`@supports` wrappers do
    /// not contribute to this (their inner rules surface with the surrounding media context).
    pub media: Option<String>,
}

/// Parse a CSS string into a [`Stylesheet`].
pub fn parse(css: &str) -> Stylesheet {
    let stripped = strip_comments(css);
    let bytes: Vec<char> = stripped.chars().collect();
    let mut rules = Vec::new();
    parse_rules(&bytes, 0, bytes.len(), None, &mut rules);
    Stylesheet { rules }
}

/// Parse the rules within `bytes[start..end]`, appending them to `out`. `media` is the media
/// query currently in scope (from enclosing `@media` blocks). At-rules with blocks (`@media`,
/// `@supports`, `@layer name { … }`) recurse; other at-rules are consumed/skipped.
fn parse_rules(
    bytes: &[char],
    start: usize,
    end: usize,
    media: Option<&str>,
    out: &mut Vec<Rule>,
) {
    let mut pos = start;
    while pos < end {
        // Skip leading whitespace.
        while pos < end && bytes[pos].is_whitespace() {
            pos += 1;
        }
        if pos >= end {
            break;
        }

        if bytes[pos] == '@' {
            pos = parse_at_rule(bytes, pos, end, media, out);
            continue;
        }

        if bytes[pos] == '}' {
            // Stray close brace; skip it.
            pos += 1;
            continue;
        }

        // Read the selector prelude up to the next top-level `{` (or end / stray `}`),
        // skipping over balanced parens and strings so `:is(a, b)` etc. don't confuse us.
        let prelude_start = pos;
        pos = scan_to_block_or_semi(bytes, pos, end);

        if pos >= end {
            // Dangling prelude with no block — ignore.
            break;
        }
        if bytes[pos] == '}' {
            // No declaration block — skip the `}` and ignore the dangling prelude.
            pos += 1;
            continue;
        }
        if bytes[pos] == ';' {
            // A statement with no block at rule level (unusual); skip it.
            pos += 1;
            continue;
        }

        let prelude: String = bytes[prelude_start..pos].iter().collect();
        // Skip the `{`.
        pos += 1;

        // Read the declaration block body up to the matching `}` (balanced).
        let body_start = pos;
        let body_end = scan_balanced_block_end(bytes, pos, end);
        let body: String = bytes[body_start..body_end].iter().collect();
        pos = if body_end < end { body_end + 1 } else { body_end };

        let selectors = parse_selector_list(&prelude);
        let declarations = parse_declarations(&body);
        if !selectors.is_empty() {
            out.push(Rule { selectors, declarations, media: media.map(str::to_string) });
        }
    }
}

/// Parse an at-rule starting at `start` (`bytes[start] == '@'`). `@media`/`@supports`/`@layer`
/// with a block recurse into their body; statement at-rules (ending in `;`) and unknown
/// block at-rules (`@font-face`, `@keyframes`, `@property`, …) are consumed and skipped.
/// Returns the position just past the at-rule.
fn parse_at_rule(
    bytes: &[char],
    start: usize,
    end: usize,
    media: Option<&str>,
    out: &mut Vec<Rule>,
) -> usize {
    // Read the at-rule name (the identifier after `@`).
    let mut i = start + 1;
    let name_start = i;
    while i < end
        && (bytes[i].is_ascii_alphanumeric() || bytes[i] == '-' || bytes[i] == '_')
    {
        i += 1;
    }
    let name: String = bytes[name_start..i].iter().collect();
    let name = name.to_ascii_lowercase();

    // Find the prelude end: the first top-level `{` or `;`.
    let prelude_start = i;
    let prelude_end = scan_to_block_or_semi(bytes, i, end);

    if prelude_end >= end {
        return prelude_end;
    }

    if bytes[prelude_end] == ';' {
        // Statement at-rule (e.g. `@layer a, b, c;`, `@import …;`, `@charset …;`). Skip.
        return prelude_end + 1;
    }

    // Block at-rule. Locate the balanced block body.
    let body_start = prelude_end + 1;
    let body_end = scan_balanced_block_end(bytes, body_start, end);
    let next = if body_end < end { body_end + 1 } else { body_end };

    let prelude: String = bytes[prelude_start..prelude_end].iter().collect();
    let prelude = prelude.trim();

    match name.as_str() {
        "media" => {
            // Combine the existing media context with this query.
            let combined = match media {
                Some(m) if !m.is_empty() => format!("{m} and {prelude}"),
                _ => prelude.to_string(),
            };
            parse_rules(bytes, body_start, body_end, Some(&combined), out);
        }
        "supports" | "layer" => {
            // Unwrap: parse the inner content as normal rules, preserving media context.
            parse_rules(bytes, body_start, body_end, media, out);
        }
        // @font-face, @keyframes, @property, @page, @counter-style, @font-feature-values, …
        // Consume and skip the block (no rules emitted).
        _ => {}
    }

    next
}

/// Scan forward from `pos` to the first top-level `{` or `;` (or `}` / `end`), skipping over
/// balanced `(…)` and `[…]`, and string literals. Returns the index of the stopping char (or
/// `end`).
fn scan_to_block_or_semi(bytes: &[char], mut pos: usize, end: usize) -> usize {
    while pos < end {
        match bytes[pos] {
            '{' | ';' | '}' => return pos,
            '(' => pos = skip_balanced(bytes, pos, end, '(', ')'),
            '[' => pos = skip_balanced(bytes, pos, end, '[', ']'),
            '"' => pos = skip_string(bytes, pos, end, '"'),
            '\'' => pos = skip_string(bytes, pos, end, '\''),
            _ => pos += 1,
        }
    }
    pos
}

/// Given `pos` pointing just inside a `{` block (at the first body char), scan to the matching
/// close `}` at the same nesting depth, skipping nested blocks, parens, and strings. Returns
/// the index of the matching `}` (or `end` if unbalanced).
fn scan_balanced_block_end(bytes: &[char], mut pos: usize, end: usize) -> usize {
    let mut depth = 0usize;
    while pos < end {
        match bytes[pos] {
            '}' => {
                if depth == 0 {
                    return pos;
                }
                depth -= 1;
                pos += 1;
            }
            '{' => {
                depth += 1;
                pos += 1;
            }
            '(' => pos = skip_balanced(bytes, pos, end, '(', ')'),
            '[' => pos = skip_balanced(bytes, pos, end, '[', ']'),
            '"' => pos = skip_string(bytes, pos, end, '"'),
            '\'' => pos = skip_string(bytes, pos, end, '\''),
            _ => pos += 1,
        }
    }
    pos
}

/// Skip a balanced `open … close` run starting at `pos` (which is `open`). Handles nesting and
/// strings inside. Returns the index just past the matching `close` (or `end`).
fn skip_balanced(bytes: &[char], mut pos: usize, end: usize, open: char, close: char) -> usize {
    let mut depth = 0usize;
    while pos < end {
        let c = bytes[pos];
        if c == open {
            depth += 1;
            pos += 1;
        } else if c == close {
            depth -= 1;
            pos += 1;
            if depth == 0 {
                return pos;
            }
        } else if c == '"' {
            pos = skip_string(bytes, pos, end, '"');
        } else if c == '\'' {
            pos = skip_string(bytes, pos, end, '\'');
        } else {
            pos += 1;
        }
    }
    pos
}

/// Skip a string literal starting at `pos` (the opening quote). Returns the index just past
/// the closing quote (or `end`). Handles backslash escapes.
fn skip_string(bytes: &[char], pos: usize, end: usize, quote: char) -> usize {
    let mut i = pos + 1;
    while i < end {
        if bytes[i] == '\\' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            return i + 1;
        }
        i += 1;
    }
    i
}

/// Parse an inline `style="..."` attribute value (or any bare declaration block) into a
/// list of `(property, value)` pairs. Property names are lowercased; values are trimmed
/// and kept as-is.
pub fn parse_declarations(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for chunk in split_top_level(s, ';') {
        let chunk = chunk.trim();
        if chunk.is_empty() {
            continue;
        }
        // Split on the FIRST top-level colon (values may contain `:` inside `url(...)` etc.).
        if let Some(idx) = find_top_level(chunk, ':') {
            let prop = chunk[..idx].trim().to_ascii_lowercase();
            let val = chunk[idx + 1..].trim().to_string();
            if !prop.is_empty() && !val.is_empty() {
                out.push((prop, val));
            }
        }
    }
    out
}

/// Split `s` on top-level occurrences of `sep` (not inside `(…)`, `[…]`, or string literals).
fn split_top_level(s: &str, sep: char) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = quote {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if c == sep && depth == 0 => {
                parts.push(chars[start..i].iter().collect());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(chars[start..].iter().collect());
    parts
}

/// Find the index (in chars-as-bytes terms via char_indices) of the first top-level `sep`.
fn find_top_level(s: &str, sep: char) -> Option<usize> {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut prev_backslash = false;
    for (idx, c) in s.char_indices() {
        if let Some(q) = quote {
            if prev_backslash {
                prev_backslash = false;
            } else if c == '\\' {
                prev_backslash = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' | '[' => depth += 1,
            ')' | ']' => depth = (depth - 1).max(0),
            _ if c == sep && depth == 0 => return Some(idx),
            _ => {}
        }
    }
    None
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
    fn media_at_rule_surfaces_tagged_rules() {
        let sheet = parse(
            "p { color: red } @media (max-width: 600px) { p { color: green } } h1 { color: blue }",
        );
        // All three rules surface now; the nested one carries its media query.
        assert_eq!(sheet.rules.len(), 3);
        assert_eq!(sheet.rules[0].selectors, vec!["p"]);
        assert_eq!(sheet.rules[0].media, None);
        assert_eq!(sheet.rules[1].selectors, vec!["p"]);
        assert_eq!(sheet.rules[1].media.as_deref(), Some("(max-width: 600px)"));
        assert_eq!(sheet.rules[1].declarations, vec![("color".to_string(), "green".to_string())]);
        assert_eq!(sheet.rules[2].selectors, vec!["h1"]);
        assert_eq!(sheet.rules[2].media, None);
    }

    #[test]
    fn layer_block_surfaces_inner_rules() {
        let sheet = parse("@layer utilities { .a { color: #f00 } }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].selectors, vec![".a"]);
        assert_eq!(sheet.rules[0].media, None);
        assert_eq!(sheet.rules[0].declarations, vec![("color".to_string(), "#f00".to_string())]);
    }

    #[test]
    fn bare_layer_statement_yields_no_rules() {
        let sheet = parse("@layer theme, base, components, utilities;");
        assert_eq!(sheet.rules.len(), 0);
    }

    #[test]
    fn nested_layer_then_media_surfaces_rule() {
        let sheet = parse("@layer base { @media (min-width: 640px) { .c { color: #0f0 } } }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].selectors, vec![".c"]);
        assert_eq!(sheet.rules[0].media.as_deref(), Some("(min-width: 640px)"));
    }

    #[test]
    fn media_min_width_tagged() {
        let sheet = parse("@media (min-width: 768px) { .b { color: #0f0 } }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].selectors, vec![".b"]);
        assert_eq!(sheet.rules[0].media.as_deref(), Some("(min-width: 768px)"));
    }

    #[test]
    fn supports_block_surfaces_inner_rules() {
        let sheet = parse("@supports (display: grid) { .g { display: grid } }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].selectors, vec![".g"]);
    }

    #[test]
    fn skipped_block_at_rules_emit_nothing() {
        let sheet = parse(
            "@font-face { font-family: x; src: url('a.woff2') } \
             @keyframes spin { from { transform: rotate(0) } to { transform: rotate(360deg) } } \
             @property --p { syntax: '<color>'; inherits: false } \
             p { color: red }",
        );
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].selectors, vec!["p"]);
    }

    #[test]
    fn stress_parens_relative_colors_and_strings_do_not_panic() {
        let css = "@layer utilities { \
            .x { color: rgb(from red r g b); background: var(--c, oklch(0.5 0.1 30)) } \
            .y::before { content: '{ ; not a brace }'; color: hsl(0 100% 50%) } \
            .z { background: url(\"data:image/svg+xml;base64,abc{}\") } \
            @media (min-width: 1024px) and (max-width: 1280px) { .w { width: calc(100% - 2rem) } } \
        }";
        let sheet = parse(css);
        // Should surface .x, .y::before, .z, and .w without panicking.
        assert!(sheet.rules.iter().any(|r| r.selectors.iter().any(|s| s == ".x")));
        assert!(sheet.rules.iter().any(|r| r.selectors.iter().any(|s| s == ".z")));
        assert!(sheet
            .rules
            .iter()
            .any(|r| r.selectors.iter().any(|s| s == ".w") && r.media.is_some()));
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
