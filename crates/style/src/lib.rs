//! Selector matching + the cascade.
//!
//! [`cascade`] walks a [`dom::Document`], matches a built-in user-agent stylesheet plus the
//! author `<style>` sheets and inline `style="…"` attributes against each element, resolves
//! the winning declarations by origin + specificity + source order, applies inheritance, and
//! returns a [`ComputedStyle`] per element [`dom::NodeId`].
//!
//! Supported selectors are *simple*: type/tag (`p`), class (`.x`), id (`#id`), the universal
//! selector (`*`), and grouped comma lists. A single compound like `p.note` (a tag plus one
//! class/id) is also handled. Descendant combinators (`div p`) are NOT supported.

use std::collections::HashMap;

/// The four sides of a box (margin / border / padding thicknesses, or content insets), in px.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Edges {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl Edges {
    /// All four sides set to the same value.
    pub fn all(v: f32) -> Self {
        Edges { top: v, right: v, bottom: v, left: v }
    }
}

/// The `display` mode of an element. Drives which layout algorithm lays out its children
/// and how the element itself participates in its parent's flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Display {
    None,
    Block,
    Inline,
    InlineBlock,
    Flex,
    InlineFlex,
    Grid,
    InlineGrid,
}

/// CSS `position`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Static,
    Relative,
    Absolute,
    Fixed,
    Sticky,
}

/// Flex container main-axis direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    RowReverse,
    Column,
    ColumnReverse,
}

/// Flex container wrapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexWrap {
    NoWrap,
    Wrap,
    WrapReverse,
}

/// Main-axis distribution of flex items / cross-axis distribution of flex lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JustifyContent {
    FlexStart,
    FlexEnd,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Cross-axis alignment of flex items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignItems {
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

/// Per-item cross-axis alignment override.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlignSelf {
    Auto,
    Stretch,
    FlexStart,
    FlexEnd,
    Center,
    Baseline,
}

/// A grid track size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackSize {
    Px(f32),
    Fr(f32),
    Pct(f32),
    Auto,
}

/// A grid line placement: `(start_line, GridSpan)`. Lines are 1-based as in CSS.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridPlacement {
    /// 1-based start line (`None` = auto-place).
    pub start: Option<i32>,
    /// End placement.
    pub end: GridEnd,
}

/// The end side of a grid placement.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridEnd {
    /// Auto (single cell unless a span widens it).
    Auto,
    /// An explicit 1-based end line.
    Line(i32),
    /// Span N tracks from the start.
    Span(i32),
}

impl Default for GridPlacement {
    fn default() -> Self {
        GridPlacement { start: None, end: GridEnd::Auto }
    }
}

/// The computed style for a single element.
#[derive(Debug, Clone, PartialEq)]
pub struct ComputedStyle {
    /// Text color (r, g, b).
    pub color: (u8, u8, u8),
    /// Background color, if any (r, g, b). `None` means transparent.
    pub background_color: Option<(u8, u8, u8)>,
    /// Font size in pixels.
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    pub text_align: TextAlign,
    /// `display: none` hides the element and its subtree. Derived from [`display`](Self::display).
    pub display_none: bool,
    /// Whether this element participates in block flow (`display: block`) vs inline.
    /// Derived from [`display`](Self::display); kept for existing readers.
    pub display_block: bool,
    /// The full display mode. Drives layout dispatch.
    pub display: Display,
    /// CSS `position`. Not inherited.
    pub position: Position,
    /// Inset `top` in px (`None` = auto). Percentages unsupported (stored as None).
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
    /// Stacking `z-index` (`None` = auto). Parsed but not yet used for paint ordering.
    pub z_index: Option<i32>,
    /// Explicit content `width` in px (`None` = auto). Percentages are ignored (None).
    pub width: Option<f32>,
    /// Explicit content `height` in px (`None` = auto).
    pub height: Option<f32>,
    /// Margin thicknesses (px). Not inherited.
    pub margin: Edges,
    /// Padding thicknesses (px). Not inherited.
    pub padding: Edges,
    /// Border *widths* (px). Not inherited.
    pub border: Edges,
    /// Border color (r, g, b).
    pub border_color: (u8, u8, u8),

    // --- Flex container properties ---
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub justify_content: JustifyContent,
    pub align_items: AlignItems,
    /// Cross-axis distribution of flex lines (multi-line). `None` = default (stretch-ish).
    pub align_content: Option<JustifyContent>,

    // --- Flex item properties ---
    pub flex_grow: f32,
    pub flex_shrink: f32,
    /// Flex basis in px (`None` = auto).
    pub flex_basis: Option<f32>,
    pub align_self: AlignSelf,
    pub order: i32,

    // --- Gaps (flex & grid) ---
    pub row_gap: f32,
    pub column_gap: f32,

    // --- Grid container properties ---
    pub grid_template_columns: Vec<TrackSize>,
    pub grid_template_rows: Vec<TrackSize>,

    // --- Grid item placement ---
    pub grid_column: Option<GridPlacement>,
    pub grid_row: Option<GridPlacement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

impl Default for ComputedStyle {
    fn default() -> Self {
        ComputedStyle {
            color: (216, 216, 216), // #d8d8d8 light grey (engine paints a dark background)
            background_color: None,
            font_size: 16.0,
            bold: false,
            italic: false,
            text_align: TextAlign::Left,
            display_none: false,
            display_block: false,
            display: Display::Inline,
            position: Position::Static,
            top: None,
            right: None,
            bottom: None,
            left: None,
            z_index: None,
            width: None,
            height: None,
            margin: Edges::default(),
            padding: Edges::default(),
            border: Edges::default(),
            border_color: (216, 216, 216), // mid/light grey, matching default text color
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            justify_content: JustifyContent::FlexStart,
            align_items: AlignItems::Stretch,
            align_content: None,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: None,
            align_self: AlignSelf::Auto,
            order: 0,
            row_gap: 0.0,
            column_gap: 0.0,
            grid_template_columns: Vec::new(),
            grid_template_rows: Vec::new(),
            grid_column: None,
            grid_row: None,
        }
    }
}

/// Compute a [`ComputedStyle`] for every element node in `doc`, using the built-in UA
/// stylesheet first, then the supplied author `sheets` (in document order), then each
/// element's inline `style="…"` attribute (highest precedence within an element).
pub fn cascade(
    doc: &dom::Document,
    sheets: &[css::Stylesheet],
) -> HashMap<dom::NodeId, ComputedStyle> {
    let ua = user_agent_stylesheet();
    let mut out = HashMap::new();
    // The root inherits from a fresh default style.
    let initial = ComputedStyle::default();
    // Custom properties (`--name`) inherit; the root starts with an empty environment.
    let initial_vars: HashMap<String, String> = HashMap::new();
    cascade_node(doc, doc.root(), &initial, &initial_vars, false, &ua, sheets, &mut out);
    out
}

/// Assumed viewport width (px) used to evaluate `min-width`/`max-width` media queries during
/// the cascade, since the real viewport isn't part of [`cascade`]'s signature.
const ASSUMED_VIEWPORT_WIDTH: f32 = 1280.0;

/// Recursively compute styles. `parent` is the parent's computed style (the inheritance
/// source); `parent_vars` is the set of custom properties inherited from ancestors;
/// `parent_hidden` is true if any ancestor was `display: none`.
#[allow(clippy::too_many_arguments)]
fn cascade_node(
    doc: &dom::Document,
    id: dom::NodeId,
    parent: &ComputedStyle,
    parent_vars: &HashMap<String, String>,
    parent_hidden: bool,
    ua: &css::Stylesheet,
    author: &[css::Stylesheet],
    out: &mut HashMap<dom::NodeId, ComputedStyle>,
) {
    let node = doc.get(id);
    let (computed, vars) = if let dom::NodeData::Element(el) = &node.data {
        let (style, vars) =
            compute_element_style(el, parent, parent_vars, parent_hidden, ua, author);
        out.insert(id, style.clone());
        (style, vars)
    } else {
        // Non-elements inherit the parent style so text runs can read color/size off the
        // nearest element ancestor via the parent passed down.
        (parent.clone(), parent_vars.clone())
    };
    let hidden = parent_hidden || computed.display_none;
    for &child in &node.children {
        cascade_node(doc, child, &computed, &vars, hidden, ua, author, out);
    }
}

/// Resolve one element's computed style: gather matching declarations from all origins in
/// precedence order, apply them, then layer inheritance.
fn compute_element_style(
    el: &dom::ElementData,
    parent: &ComputedStyle,
    parent_vars: &HashMap<String, String>,
    parent_hidden: bool,
    ua: &css::Stylesheet,
    author: &[css::Stylesheet],
) -> (ComputedStyle, HashMap<String, String>) {
    // Start from inherited values; non-inherited properties get reset below.
    let mut style = ComputedStyle {
        color: parent.color,
        background_color: None, // not inherited
        font_size: parent.font_size,
        bold: parent.bold,
        italic: parent.italic,
        text_align: parent.text_align,
        display_none: false, // not inherited
        display_block: false,
        display: Display::Inline,
        position: Position::Static,
        top: None,
        right: None,
        bottom: None,
        left: None,
        z_index: None,
        // Box properties are not inherited: each element starts from initial values.
        width: None,
        height: None,
        margin: Edges::default(),
        padding: Edges::default(),
        border: Edges::default(),
        border_color: parent.color, // initial border-color is currentColor
        flex_direction: FlexDirection::Row,
        flex_wrap: FlexWrap::NoWrap,
        justify_content: JustifyContent::FlexStart,
        align_items: AlignItems::Stretch,
        align_content: None,
        flex_grow: 0.0,
        flex_shrink: 1.0,
        flex_basis: None,
        align_self: AlignSelf::Auto,
        order: 0,
        row_gap: 0.0,
        column_gap: 0.0,
        grid_template_columns: Vec::new(),
        grid_template_rows: Vec::new(),
        grid_column: None,
        grid_row: None,
    };
    if parent_hidden {
        style.display_none = true;
        style.display = Display::None;
    }

    // Collect (specificity, source_order, declarations) from every matching rule across all
    // origins. We process origins lowest-precedence-first and rely on a stable sort that puts
    // later, higher-specificity entries last so they win when applied in order.
    struct MatchEntry<'a> {
        origin: u8, // 0 = UA, 1 = author, 2 = inline
        specificity: u32,
        order: usize,
        decls: &'a [(String, String)],
    }
    let mut matches: Vec<MatchEntry> = Vec::new();
    let mut order = 0usize;

    for rule in &ua.rules {
        if media_applies(rule.media.as_deref()) {
            if let Some(spec) = rule_specificity(&rule.selectors, el) {
                matches.push(MatchEntry { origin: 0, specificity: spec, order, decls: &rule.declarations });
            }
        }
        order += 1;
    }
    for sheet in author {
        for rule in &sheet.rules {
            if media_applies(rule.media.as_deref()) {
                if let Some(spec) = rule_specificity(&rule.selectors, el) {
                    matches.push(MatchEntry { origin: 1, specificity: spec, order, decls: &rule.declarations });
                }
            }
            order += 1;
        }
    }

    // Inline style is its own origin with highest precedence.
    let inline_decls: Vec<(String, String)> = el
        .attrs
        .get("style")
        .map(|s| css::parse_declarations(s))
        .unwrap_or_default();
    if !inline_decls.is_empty() {
        matches.push(MatchEntry { origin: 2, specificity: 0, order, decls: &inline_decls });
    }

    // Sort by (origin, specificity, order) ascending so the winner is applied last.
    matches.sort_by(|a, b| {
        a.origin
            .cmp(&b.origin)
            .then(a.specificity.cmp(&b.specificity))
            .then(a.order.cmp(&b.order))
    });

    // Build this element's custom-property environment: inherit the ancestors' vars, then
    // override with any `--name: value` declared on this element (in cascade order, so the
    // winning declaration applies last).
    let mut vars = parent_vars.clone();
    for m in &matches {
        for (prop, val) in m.decls {
            if let Some(name) = prop.strip_prefix("--") {
                vars.insert(format!("--{name}"), val.clone());
            }
        }
    }

    // Now apply the regular declarations, resolving any `var(...)` references against `vars`
    // and supplying the current/inherited color for `currentColor`/`inherit`.
    let inherited_color = parent.color;
    for m in &matches {
        for (prop, val) in m.decls {
            if prop.starts_with("--") {
                continue; // custom properties are environment, not applied directly
            }
            let resolved = resolve_vars(val, &vars);
            let current_color = style.color;
            apply_declaration(&mut style, prop, &resolved, parent, current_color, inherited_color);
        }
    }

    // The UA stylesheet emits `display: block` for block tags; everything else defaults to
    // inline. If no author/UA rule set a display, fall back to the per-tag default.
    let display_was_set = matches.iter().any(|m| {
        m.decls.iter().any(|(p, _)| p.eq_ignore_ascii_case("display"))
    });
    if !display_was_set && style.display == Display::Inline && is_block_tag(&el.tag) {
        style.display = Display::Block;
    }
    if parent_hidden {
        style.display = Display::None;
    }

    // Keep the legacy derived flags in sync for existing readers (engine / layout fallbacks).
    style.display_none = style.display == Display::None;
    style.display_block = matches!(
        style.display,
        Display::Block | Display::Flex | Display::Grid | Display::None
    );

    (style, vars)
}

/// Resolve `var(--name, fallback)` references in `value` against `vars`, recursively (vars can
/// reference vars). Bounded against cyclic references by a recursion-depth cap.
fn resolve_vars(value: &str, vars: &HashMap<String, String>) -> String {
    resolve_vars_depth(value, vars, 0)
}

const VAR_MAX_DEPTH: usize = 32;

fn resolve_vars_depth(value: &str, vars: &HashMap<String, String>, depth: usize) -> String {
    if depth >= VAR_MAX_DEPTH || !value.contains("var(") {
        return value.to_string();
    }
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    while i < chars.len() {
        // Detect `var(` at a token boundary.
        if chars[i] == 'v'
            && chars[i..].len() >= 4
            && chars[i + 1] == 'a'
            && chars[i + 2] == 'r'
            && chars[i + 3] == '('
        {
            // Find the matching close paren for this `var(`.
            let args_start = i + 4;
            let mut j = args_start;
            let mut pdepth = 1i32;
            while j < chars.len() && pdepth > 0 {
                match chars[j] {
                    '(' => pdepth += 1,
                    ')' => pdepth -= 1,
                    _ => {}
                }
                if pdepth == 0 {
                    break;
                }
                j += 1;
            }
            // chars[j] is the matching ')'.
            let args: String = chars[args_start..j].iter().collect();
            let replacement = resolve_one_var(&args, vars, depth);
            out.push_str(&replacement);
            i = j + 1; // skip past ')'
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Resolve the args of a single `var(...)`: `--name` or `--name, fallback`. Returns the
/// resolved (and recursively var-expanded) value, or the (expanded) fallback, or empty.
fn resolve_one_var(args: &str, vars: &HashMap<String, String>, depth: usize) -> String {
    // Split into name and optional fallback at the first top-level comma.
    let (name, fallback) = split_first_comma(args);
    let name = name.trim();
    if let Some(v) = vars.get(name) {
        // The looked-up value may itself contain var() references.
        return resolve_vars_depth(v, vars, depth + 1);
    }
    match fallback {
        Some(fb) => resolve_vars_depth(fb.trim(), vars, depth + 1),
        None => String::new(),
    }
}

/// Split `s` at the first top-level comma (not inside nested parens). Returns `(before, after)`.
fn split_first_comma(s: &str) -> (&str, Option<&str>) {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    for (idx, &b) in bytes.iter().enumerate() {
        match b {
            b'(' => depth += 1,
            b')' => depth -= 1,
            b',' if depth == 0 => return (&s[..idx], Some(&s[idx + 1..])),
            _ => {}
        }
    }
    (s, None)
}

/// Decide whether a rule with the given raw `@media` query applies at the assumed desktop
/// viewport ([`ASSUMED_VIEWPORT_WIDTH`]). `None` (no media) always applies. We parse the
/// common Tailwind shapes: `screen`/`all` match, `print` does not, and single
/// `min-width`/`max-width` px thresholds are compared against the assumed width. Multiple
/// `and`-joined conditions must all pass. Unrecognized features are treated as matching
/// (best-effort, so we don't drop rules we can't fully parse).
fn media_applies(media: Option<&str>) -> bool {
    let query = match media {
        None => return true,
        Some(q) => q.trim(),
    };
    if query.is_empty() {
        return true;
    }
    // A comma-separated media query list matches if ANY component matches.
    query.split(',').any(|component| media_component_matches(component))
}

fn media_component_matches(component: &str) -> bool {
    let lower = component.trim().to_ascii_lowercase();
    // Split on `and`; each part is a media type or a `(feature: value)` condition.
    for raw in lower.split(" and ") {
        let part = raw.trim();
        if part.is_empty() {
            continue;
        }
        // Media types.
        if part == "screen" || part == "all" {
            continue;
        }
        if part == "print" {
            return false;
        }
        // Feature conditions like `(min-width: 768px)`.
        if let Some(inner) = part.strip_prefix('(').and_then(|p| p.strip_suffix(')')) {
            if let Some((feature, value)) = inner.split_once(':') {
                let feature = feature.trim();
                let value = value.trim();
                match feature {
                    "min-width" => {
                        if let Some(px) = length_px(value) {
                            if ASSUMED_VIEWPORT_WIDTH < px {
                                return false;
                            }
                        }
                    }
                    "max-width" => {
                        if let Some(px) = length_px(value) {
                            if ASSUMED_VIEWPORT_WIDTH > px {
                                return false;
                            }
                        }
                    }
                    // Unrecognized features (orientation, prefers-*, …): treat as matching.
                    _ => {}
                }
            }
            continue;
        }
        // Bare `not`/`only` prefixes or unknown tokens: be permissive (treat as matching),
        // except an explicit leading `not` which we honor crudely.
        if part.starts_with("not ") {
            return false;
        }
    }
    true
}

/// Parse a media-query length to px. Supports `px`, `rem`/`em` (×16), bare numbers (px).
fn length_px(value: &str) -> Option<f32> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(n) = v.strip_suffix("px") {
        n.trim().parse::<f32>().ok()
    } else if let Some(n) = v.strip_suffix("rem") {
        n.trim().parse::<f32>().ok().map(|x| x * 16.0)
    } else if let Some(n) = v.strip_suffix("em") {
        n.trim().parse::<f32>().ok().map(|x| x * 16.0)
    } else {
        v.parse::<f32>().ok()
    }
}

/// Block-level-by-default tags (mirrors the layout UA list).
fn is_block_tag(tag: &str) -> bool {
    matches!(
        tag.to_ascii_lowercase().as_str(),
        "html" | "body" | "div" | "p" | "section" | "article" | "header" | "footer" | "nav"
            | "main" | "aside" | "ul" | "ol" | "li" | "blockquote" | "pre" | "table" | "tr"
            | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "form" | "fieldset" | "figure"
            | "figcaption" | "address" | "hr"
    )
}

/// Parse a `justify-content` / `align-content` keyword.
fn parse_justify(val: &str) -> Option<JustifyContent> {
    match val.trim().to_ascii_lowercase().as_str() {
        "flex-start" | "start" | "left" => Some(JustifyContent::FlexStart),
        "flex-end" | "end" | "right" => Some(JustifyContent::FlexEnd),
        "center" => Some(JustifyContent::Center),
        "space-between" => Some(JustifyContent::SpaceBetween),
        "space-around" => Some(JustifyContent::SpaceAround),
        "space-evenly" => Some(JustifyContent::SpaceEvenly),
        _ => None,
    }
}

/// Parse the `flex` shorthand. Supported forms:
/// - `none` → grow 0, shrink 0, basis auto
/// - `auto` → grow 1, shrink 1, basis auto
/// - a single number `N` → grow N, shrink 1, basis 0
/// - `grow shrink basis` (2 or 3 tokens; unitless numbers are grow/shrink, a length is basis)
fn apply_flex_shorthand(style: &mut ComputedStyle, val: &str) {
    let v = val.trim().to_ascii_lowercase();
    if v == "none" {
        style.flex_grow = 0.0;
        style.flex_shrink = 0.0;
        style.flex_basis = None;
        return;
    }
    if v == "auto" {
        style.flex_grow = 1.0;
        style.flex_shrink = 1.0;
        style.flex_basis = None;
        return;
    }
    if v == "initial" {
        style.flex_grow = 0.0;
        style.flex_shrink = 1.0;
        style.flex_basis = None;
        return;
    }

    let toks: Vec<&str> = val.split_whitespace().collect();
    // Classify tokens: unitless numbers (no %/px/unit) are grow then shrink; anything that
    // parses as a length (or `auto`/0px) is the basis.
    let mut nums: Vec<f32> = Vec::new();
    let mut basis: Option<Option<f32>> = None; // Some(None)=auto, Some(Some(x))=px
    for t in &toks {
        let tl = t.to_ascii_lowercase();
        if tl == "auto" {
            basis = Some(None);
            continue;
        }
        // A bare unitless integer/float is a flex factor; a value with a unit/% is the basis.
        let has_unit = tl.ends_with("px")
            || tl.ends_with('%')
            || tl.ends_with("pt")
            || tl.ends_with("em")
            || tl.ends_with("rem");
        if !has_unit {
            if let Ok(n) = tl.parse::<f32>() {
                if nums.len() < 2 {
                    nums.push(n);
                    continue;
                }
            }
        }
        // Otherwise treat as a length basis.
        basis = Some(parse_length(t));
    }
    match nums.len() {
        0 => {}
        1 => {
            style.flex_grow = nums[0].max(0.0);
            style.flex_shrink = 1.0;
            // `flex: 1` → basis 0 unless an explicit basis was given.
            if basis.is_none() {
                style.flex_basis = Some(0.0);
            }
        }
        _ => {
            style.flex_grow = nums[0].max(0.0);
            style.flex_shrink = nums[1].max(0.0);
            if basis.is_none() {
                style.flex_basis = Some(0.0);
            }
        }
    }
    if let Some(b) = basis {
        style.flex_basis = b;
    }
}

/// Parse a `gap` value: 1 value → both row & column; 2 values → row column.
fn parse_gap(val: &str) -> Option<(f32, f32)> {
    let parts: Vec<f32> = val
        .split_whitespace()
        .filter_map(parse_length)
        .collect();
    match parts.len() {
        1 => Some((parts[0], parts[0])),
        n if n >= 2 => Some((parts[0], parts[1])),
        _ => None,
    }
}

/// Parse a space-separated grid track list. Supports `Npx`, `Nfr`, `N%`, `auto`, and
/// `repeat(n, <track>)` (expanded). Unrecognized tokens are skipped.
fn parse_track_list(val: &str) -> Vec<TrackSize> {
    let mut out = Vec::new();
    let lower = val.trim().to_ascii_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        // Read a token up to whitespace, but keep balanced parens together (for repeat()).
        let start = i;
        let mut depth = 0i32;
        while i < chars.len() {
            let c = chars[i];
            if c == '(' {
                depth += 1;
            } else if c == ')' {
                depth -= 1;
            } else if c.is_whitespace() && depth == 0 {
                break;
            }
            i += 1;
        }
        let tok: String = chars[start..i].iter().collect();
        if let Some(inner) = tok.strip_prefix("repeat(").and_then(|s| s.strip_suffix(')')) {
            // repeat(count, tracks...)
            if let Some((count_s, rest)) = inner.split_once(',') {
                if let Ok(count) = count_s.trim().parse::<usize>() {
                    let inner_tracks = parse_track_list(rest);
                    for _ in 0..count.min(1000) {
                        out.extend(inner_tracks.iter().copied());
                    }
                }
            }
        } else if let Some(t) = parse_track_size(&tok) {
            out.push(t);
        }
    }
    out
}

/// Parse a single grid track size token.
fn parse_track_size(tok: &str) -> Option<TrackSize> {
    let t = tok.trim();
    if t == "auto" {
        return Some(TrackSize::Auto);
    }
    if let Some(fr) = t.strip_suffix("fr") {
        return fr.trim().parse::<f32>().ok().map(TrackSize::Fr);
    }
    if let Some(pct) = t.strip_suffix('%') {
        return pct.trim().parse::<f32>().ok().map(TrackSize::Pct);
    }
    parse_length(t).map(TrackSize::Px)
}

/// Parse a `grid-column` / `grid-row` placement: `a`, `a / b`, `a / span N`, or `span N`.
fn parse_grid_placement(val: &str) -> Option<GridPlacement> {
    let v = val.trim().to_ascii_lowercase();
    if v.is_empty() || v == "auto" {
        return None;
    }
    let (start_s, end_s) = match v.split_once('/') {
        Some((a, b)) => (a.trim(), Some(b.trim())),
        None => (v.as_str(), None),
    };

    // Parse the start side. It may itself be a `span N`.
    let (start, span_from_start) = if let Some(n) = start_s.strip_prefix("span") {
        (None, n.trim().parse::<i32>().ok())
    } else {
        (start_s.parse::<i32>().ok(), None)
    };

    let end = match end_s {
        None => {
            if let Some(s) = span_from_start {
                GridEnd::Span(s)
            } else {
                GridEnd::Auto
            }
        }
        Some(e) => {
            if let Some(n) = e.strip_prefix("span") {
                n.trim().parse::<i32>().ok().map(GridEnd::Span).unwrap_or(GridEnd::Auto)
            } else if let Ok(line) = e.parse::<i32>() {
                GridEnd::Line(line)
            } else {
                GridEnd::Auto
            }
        }
    };

    if start.is_none() && matches!(end, GridEnd::Auto) {
        return None;
    }
    Some(GridPlacement { start, end })
}

/// Apply a single declaration to `style`. Unknown properties/values are ignored silently.
fn apply_declaration(
    style: &mut ComputedStyle,
    prop: &str,
    val: &str,
    parent: &ComputedStyle,
    current_color: (u8, u8, u8),
    inherited_color: (u8, u8, u8),
) {
    match prop {
        "color" => {
            let trimmed = val.trim().to_ascii_lowercase();
            if trimmed == "inherit" {
                style.color = inherited_color;
            } else if trimmed == "initial" || trimmed == "unset" {
                style.color = ComputedStyle::default().color;
            } else if let Some(c) = parse_color_ctx(val, current_color, inherited_color) {
                style.color = c;
            }
        }
        "background-color" | "background" => {
            // For `background`, only attempt a solid-color interpretation; gradients/images
            // and `transparent`/`none` leave the background unchanged (None stays None).
            if let Some(c) = parse_color_ctx(val, current_color, inherited_color) {
                style.background_color = Some(c);
            }
        }
        "font-size" => {
            if let Some(sz) = parse_font_size(val, parent.font_size) {
                style.font_size = sz;
            }
        }
        "font-weight" => match parse_font_weight(val) {
            Some(b) => style.bold = b,
            None => {}
        },
        "font-style" => match val.trim().to_ascii_lowercase().as_str() {
            "italic" | "oblique" => style.italic = true,
            "normal" => style.italic = false,
            _ => {}
        },
        "text-align" => match val.trim().to_ascii_lowercase().as_str() {
            "left" => style.text_align = TextAlign::Left,
            "center" => style.text_align = TextAlign::Center,
            "right" => style.text_align = TextAlign::Right,
            _ => {}
        },
        "display" => match val.trim().to_ascii_lowercase().as_str() {
            "none" => style.display = Display::None,
            "block" => style.display = Display::Block,
            "inline" => style.display = Display::Inline,
            "inline-block" => style.display = Display::InlineBlock,
            "flex" => style.display = Display::Flex,
            "inline-flex" => style.display = Display::InlineFlex,
            "grid" => style.display = Display::Grid,
            "inline-grid" => style.display = Display::InlineGrid,
            _ => {}
        },
        "position" => match val.trim().to_ascii_lowercase().as_str() {
            "static" => style.position = Position::Static,
            "relative" => style.position = Position::Relative,
            "absolute" => style.position = Position::Absolute,
            "fixed" => style.position = Position::Fixed,
            "sticky" => style.position = Position::Sticky,
            _ => {}
        },
        "top" => style.top = parse_length(val),
        "right" => style.right = parse_length(val),
        "bottom" => style.bottom = parse_length(val),
        "left" => style.left = parse_length(val),
        "z-index" => {
            let v = val.trim().to_ascii_lowercase();
            if v == "auto" {
                style.z_index = None;
            } else if let Ok(n) = v.parse::<i32>() {
                style.z_index = Some(n);
            }
        }

        // --- Flex container ---
        "flex-direction" => match val.trim().to_ascii_lowercase().as_str() {
            "row" => style.flex_direction = FlexDirection::Row,
            "row-reverse" => style.flex_direction = FlexDirection::RowReverse,
            "column" => style.flex_direction = FlexDirection::Column,
            "column-reverse" => style.flex_direction = FlexDirection::ColumnReverse,
            _ => {}
        },
        "flex-wrap" => match val.trim().to_ascii_lowercase().as_str() {
            "nowrap" => style.flex_wrap = FlexWrap::NoWrap,
            "wrap" => style.flex_wrap = FlexWrap::Wrap,
            "wrap-reverse" => style.flex_wrap = FlexWrap::WrapReverse,
            _ => {}
        },
        "flex-flow" => {
            // shorthand: direction and/or wrap, space separated, order-insensitive.
            for tok in val.split_whitespace() {
                let t = tok.to_ascii_lowercase();
                match t.as_str() {
                    "row" => style.flex_direction = FlexDirection::Row,
                    "row-reverse" => style.flex_direction = FlexDirection::RowReverse,
                    "column" => style.flex_direction = FlexDirection::Column,
                    "column-reverse" => style.flex_direction = FlexDirection::ColumnReverse,
                    "nowrap" => style.flex_wrap = FlexWrap::NoWrap,
                    "wrap" => style.flex_wrap = FlexWrap::Wrap,
                    "wrap-reverse" => style.flex_wrap = FlexWrap::WrapReverse,
                    _ => {}
                }
            }
        }
        "justify-content" => {
            if let Some(j) = parse_justify(val) {
                style.justify_content = j;
            }
        }
        "align-items" => match val.trim().to_ascii_lowercase().as_str() {
            "stretch" => style.align_items = AlignItems::Stretch,
            "flex-start" | "start" => style.align_items = AlignItems::FlexStart,
            "flex-end" | "end" => style.align_items = AlignItems::FlexEnd,
            "center" => style.align_items = AlignItems::Center,
            "baseline" => style.align_items = AlignItems::Baseline,
            _ => {}
        },
        "align-content" => {
            if let Some(j) = parse_justify(val) {
                style.align_content = Some(j);
            }
        }

        // --- Flex item ---
        "flex" => apply_flex_shorthand(style, val),
        "flex-grow" => {
            if let Ok(n) = val.trim().parse::<f32>() {
                style.flex_grow = n.max(0.0);
            }
        }
        "flex-shrink" => {
            if let Ok(n) = val.trim().parse::<f32>() {
                style.flex_shrink = n.max(0.0);
            }
        }
        "flex-basis" => {
            let v = val.trim().to_ascii_lowercase();
            style.flex_basis = if v == "auto" { None } else { parse_length(val) };
        }
        "align-self" => match val.trim().to_ascii_lowercase().as_str() {
            "auto" => style.align_self = AlignSelf::Auto,
            "stretch" => style.align_self = AlignSelf::Stretch,
            "flex-start" | "start" => style.align_self = AlignSelf::FlexStart,
            "flex-end" | "end" => style.align_self = AlignSelf::FlexEnd,
            "center" => style.align_self = AlignSelf::Center,
            "baseline" => style.align_self = AlignSelf::Baseline,
            _ => {}
        },
        "order" => {
            if let Ok(n) = val.trim().parse::<i32>() {
                style.order = n;
            }
        }

        // --- Gaps ---
        "gap" => {
            if let Some((r, c)) = parse_gap(val) {
                style.row_gap = r;
                style.column_gap = c;
            }
        }
        "row-gap" => {
            if let Some(v) = parse_length(val) {
                style.row_gap = v;
            }
        }
        "column-gap" => {
            if let Some(v) = parse_length(val) {
                style.column_gap = v;
            }
        }

        // --- Grid ---
        "grid-template-columns" => {
            let tracks = parse_track_list(val);
            if !tracks.is_empty() {
                style.grid_template_columns = tracks;
            }
        }
        "grid-template-rows" => {
            let tracks = parse_track_list(val);
            if !tracks.is_empty() {
                style.grid_template_rows = tracks;
            }
        }
        "grid-column" => {
            style.grid_column = parse_grid_placement(val);
        }
        "grid-row" => {
            style.grid_row = parse_grid_placement(val);
        }

        // --- Box model: margin ---
        "margin" => {
            if let Some(e) = parse_edges_shorthand(val) {
                style.margin = e;
            }
        }
        "margin-top" => set_edge(&mut style.margin, EdgeSide::Top, val),
        "margin-right" => set_edge(&mut style.margin, EdgeSide::Right, val),
        "margin-bottom" => set_edge(&mut style.margin, EdgeSide::Bottom, val),
        "margin-left" => set_edge(&mut style.margin, EdgeSide::Left, val),

        // --- Box model: padding ---
        "padding" => {
            if let Some(e) = parse_edges_shorthand(val) {
                style.padding = e;
            }
        }
        "padding-top" => set_edge(&mut style.padding, EdgeSide::Top, val),
        "padding-right" => set_edge(&mut style.padding, EdgeSide::Right, val),
        "padding-bottom" => set_edge(&mut style.padding, EdgeSide::Bottom, val),
        "padding-left" => set_edge(&mut style.padding, EdgeSide::Left, val),

        // --- Box model: border ---
        "border" => apply_border_shorthand(style, val, EdgeSide::All, current_color, inherited_color),
        "border-top" => apply_border_shorthand(style, val, EdgeSide::Top, current_color, inherited_color),
        "border-right" => apply_border_shorthand(style, val, EdgeSide::Right, current_color, inherited_color),
        "border-bottom" => apply_border_shorthand(style, val, EdgeSide::Bottom, current_color, inherited_color),
        "border-left" => apply_border_shorthand(style, val, EdgeSide::Left, current_color, inherited_color),
        "border-width" => {
            if let Some(e) = parse_edges_shorthand(val) {
                style.border = e;
            }
        }
        "border-top-width" => set_edge(&mut style.border, EdgeSide::Top, val),
        "border-right-width" => set_edge(&mut style.border, EdgeSide::Right, val),
        "border-bottom-width" => set_edge(&mut style.border, EdgeSide::Bottom, val),
        "border-left-width" => set_edge(&mut style.border, EdgeSide::Left, val),
        "border-color" => {
            if let Some(c) = parse_color_ctx(val, current_color, inherited_color) {
                style.border_color = c;
            }
        }

        // --- Box model: width / height ---
        "width" => {
            style.width = parse_length(val);
        }
        "height" => {
            style.height = parse_length(val);
        }

        _ => {}
    }
}

/// Which side(s) of a box a value targets.
#[derive(Clone, Copy)]
enum EdgeSide {
    Top,
    Right,
    Bottom,
    Left,
    All,
}

/// Parse a CSS length to px. Accepts `Npx`, `Npt` (×4/3), and bare numbers (px). `auto`,
/// percentages, and unparseable values yield `None`. `0` (unitless) yields `Some(0)`.
fn parse_length(val: &str) -> Option<f32> {
    let v = val.trim().to_ascii_lowercase();
    if v.is_empty() || v == "auto" {
        return None;
    }
    if v.ends_with('%') {
        return None; // percentages unsupported for now
    }
    let num = |suffix: &str| v.strip_suffix(suffix).and_then(|n| n.trim().parse::<f32>().ok());
    if let Some(px) = num("px") {
        Some(px)
    } else if let Some(pt) = num("pt") {
        Some(pt * 4.0 / 3.0)
    } else {
        v.parse::<f32>().ok()
    }
}

/// Parse a length for an *edge* (margin/padding/border-width). Like [`parse_length`] but
/// `auto` → 0 (margin auto is not supported; treated as 0). Unparseable → `None` (leave as-is).
fn parse_edge_length(val: &str) -> Option<f32> {
    let v = val.trim().to_ascii_lowercase();
    if v == "auto" {
        return Some(0.0); // limitation: margin/padding `auto` collapses to 0
    }
    if v == "none" {
        return Some(0.0);
    }
    parse_length(val)
}

/// Set one side of an `Edges` from a single length value (ignored if unparseable).
fn set_edge(edges: &mut Edges, side: EdgeSide, val: &str) {
    if let Some(px) = parse_edge_length(val) {
        match side {
            EdgeSide::Top => edges.top = px,
            EdgeSide::Right => edges.right = px,
            EdgeSide::Bottom => edges.bottom = px,
            EdgeSide::Left => edges.left = px,
            EdgeSide::All => *edges = Edges::all(px),
        }
    }
}

/// Parse a `margin`/`padding`/`border-width` shorthand of 1–4 values.
/// CSS order: `all` / `vert horiz` / `top horiz bottom` / `top right bottom left`.
/// Returns `None` if no token parsed (leaves the existing value untouched).
fn parse_edges_shorthand(val: &str) -> Option<Edges> {
    let parts: Vec<f32> = val
        .split_whitespace()
        .map(|t| parse_edge_length(t).unwrap_or(0.0))
        .collect();
    match parts.len() {
        1 => Some(Edges::all(parts[0])),
        2 => Some(Edges { top: parts[0], bottom: parts[0], right: parts[1], left: parts[1] }),
        3 => Some(Edges {
            top: parts[0],
            right: parts[1],
            left: parts[1],
            bottom: parts[2],
        }),
        n if n >= 4 => Some(Edges {
            top: parts[0],
            right: parts[1],
            bottom: parts[2],
            left: parts[3],
        }),
        _ => None,
    }
}

/// Apply a `border` (or per-side `border-top` etc.) shorthand: extract a width (the first
/// length token; `none`/`0` → 0) and a color (the first parseable color token). Border style
/// is ignored. Tokens that are neither are skipped.
fn apply_border_shorthand(
    style: &mut ComputedStyle,
    val: &str,
    side: EdgeSide,
    current_color: (u8, u8, u8),
    inherited_color: (u8, u8, u8),
) {
    let mut width: Option<f32> = None;
    let mut color: Option<(u8, u8, u8)> = None;
    let mut saw_none = false;
    for tok in val.split_whitespace() {
        let lower = tok.to_ascii_lowercase();
        if lower == "none" || lower == "hidden" {
            saw_none = true;
            continue;
        }
        // Border-style keywords carry no geometry; skip them.
        if matches!(
            lower.as_str(),
            "solid" | "dashed" | "dotted" | "double" | "groove" | "ridge" | "inset" | "outset"
        ) {
            continue;
        }
        if width.is_none() {
            if let Some(px) = parse_length(tok) {
                width = Some(px);
                continue;
            }
        }
        if color.is_none() {
            if let Some(c) = parse_color_ctx(tok, current_color, inherited_color) {
                color = Some(c);
            }
        }
    }
    let w = if saw_none && width.is_none() { Some(0.0) } else { width };
    if let Some(w) = w {
        match side {
            EdgeSide::Top => style.border.top = w,
            EdgeSide::Right => style.border.right = w,
            EdgeSide::Bottom => style.border.bottom = w,
            EdgeSide::Left => style.border.left = w,
            EdgeSide::All => style.border = Edges::all(w),
        }
    }
    if let Some(c) = color {
        style.border_color = c;
    }
}

/// Parse a `font-weight` value: `bold` / `bolder` / numeric `>= 600` → true; `normal` /
/// `lighter` / numeric `< 600` → false; unknown → `None` (leave inherited).
fn parse_font_weight(val: &str) -> Option<bool> {
    let v = val.trim().to_ascii_lowercase();
    match v.as_str() {
        "bold" | "bolder" => Some(true),
        "normal" | "lighter" => Some(false),
        other => other.parse::<u32>().ok().map(|n| n >= 600),
    }
}

/// Parse a `font-size`: `Npx`, `Npt` (×4/3), or `Nem` (relative to `parent_px`). Bare numbers
/// are treated as px.
fn parse_font_size(val: &str, parent_px: f32) -> Option<f32> {
    let v = val.trim().to_ascii_lowercase();
    let num = |suffix: &str| v.strip_suffix(suffix).and_then(|n| n.trim().parse::<f32>().ok());
    if let Some(px) = num("px") {
        Some(px)
    } else if let Some(pt) = num("pt") {
        Some(pt * 4.0 / 3.0)
    } else if let Some(em) = num("em") {
        Some(em * parent_px)
    } else if let Some(rem) = num("rem") {
        Some(rem * 16.0)
    } else {
        v.parse::<f32>().ok().filter(|n| *n > 0.0)
    }
}

/// Parse a color to opaque `(r, g, b)`. Supports hex (`#rgb`/`#rgba`/`#rrggbb`/`#rrggbbaa`),
/// named colors, and the functional forms `rgb()`/`rgba()`, `hsl()`/`hsla()`, `oklch()`, and
/// `oklab()`. Alpha is parsed but dropped (treated as opaque). Returns `None` if unrecognized.
///
/// `current` supplies the value of `currentColor`; `inherited` supplies the value used for
/// `inherit`. Keywords `transparent`/`initial` return `None` (caller treats as "no change /
/// no color").
fn parse_color_ctx(
    val: &str,
    current: (u8, u8, u8),
    inherited: (u8, u8, u8),
) -> Option<(u8, u8, u8)> {
    let v = val.trim();
    let lower = v.to_ascii_lowercase();

    // Keywords.
    match lower.as_str() {
        "currentcolor" => return Some(current),
        "inherit" => return Some(inherited),
        "transparent" | "initial" | "unset" | "none" | "revert" => return None,
        _ => {}
    }

    if let Some(hex) = v.strip_prefix('#') {
        return parse_hex(hex);
    }

    // Functional color: name( args ).
    if let Some(open) = v.find('(') {
        if v.ends_with(')') {
            let func = v[..open].trim().to_ascii_lowercase();
            let inner = &v[open + 1..v.len() - 1];
            return parse_color_function(&func, inner);
        }
    }

    parse_named_color(&lower)
}

/// Convenience wrapper used where no element context is needed (currentColor/inherit map to a
/// neutral default). Prefer [`parse_color_ctx`] in the cascade.
#[cfg(test)]
fn parse_color(val: &str) -> Option<(u8, u8, u8)> {
    parse_color_ctx(val, (0, 0, 0), (0, 0, 0))
}

/// Parse a functional color body (the text between the parens), given the lowercased function
/// name. Handles `rgb`/`rgba`/`hsl`/`hsla`/`oklch`/`oklab`.
fn parse_color_function(func: &str, inner: &str) -> Option<(u8, u8, u8)> {
    // Relative-color syntax (`rgb(from red r g b)`) and other exotic forms are not supported;
    // bail out so the caller can fall back rather than mis-parse.
    if inner.trim_start().to_ascii_lowercase().starts_with("from ") {
        return None;
    }
    // Split on commas and/or whitespace; also strip an optional `/ alpha` segment.
    let main = inner.split('/').next().unwrap_or(inner);
    let toks: Vec<&str> = main
        .split([',', ' ', '\t', '\n'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    match func {
        "rgb" | "rgba" => {
            if toks.len() < 3 {
                return None;
            }
            Some((
                parse_rgb_component(toks[0])?,
                parse_rgb_component(toks[1])?,
                parse_rgb_component(toks[2])?,
            ))
        }
        "hsl" | "hsla" => {
            if toks.len() < 3 {
                return None;
            }
            let h = parse_number(toks[0])?;
            let s = parse_percent_or_unit(toks[1])?; // 0..1
            let l = parse_percent_or_unit(toks[2])?; // 0..1
            Some(hsl_to_rgb(h, s, l))
        }
        "oklch" => {
            if toks.len() < 3 {
                return None;
            }
            let l = parse_percent_or_unit(toks[0])?; // 0..1 (or %)
            let c = parse_number(toks[1])?;
            let h = parse_number(toks[2])?;
            Some(oklch_to_srgb(l, c, h))
        }
        "oklab" => {
            if toks.len() < 3 {
                return None;
            }
            let l = parse_percent_or_unit(toks[0])?;
            let a = parse_number(toks[1])?;
            let b = parse_number(toks[2])?;
            Some(oklab_to_srgb(l, a, b))
        }
        _ => None,
    }
}

/// Parse an rgb channel: `0..255` integer/float, or a percentage `0%..100%`.
fn parse_rgb_component(tok: &str) -> Option<u8> {
    if let Some(p) = tok.strip_suffix('%') {
        let pct = p.trim().parse::<f32>().ok()?;
        return Some((pct / 100.0 * 255.0).round().clamp(0.0, 255.0) as u8);
    }
    let n = tok.parse::<f32>().ok()?;
    Some(n.round().clamp(0.0, 255.0) as u8)
}

/// Parse a bare number (drops a trailing `deg`/`rad`/`turn` unit on angles, treating the value
/// as already in the natural unit for the caller — degrees for hue, etc.).
fn parse_number(tok: &str) -> Option<f32> {
    let t = tok.trim();
    for unit in ["deg", "grad", "rad", "turn"] {
        if let Some(stripped) = t.strip_suffix(unit) {
            let v = stripped.trim().parse::<f32>().ok()?;
            return Some(match unit {
                "deg" => v,
                "grad" => v * 0.9,
                "rad" => v.to_degrees(),
                "turn" => v * 360.0,
                _ => v,
            });
        }
    }
    t.parse::<f32>().ok()
}

/// Parse a value that may be a percentage (`50%` → 0.5) or a unitless number used as-is.
fn parse_percent_or_unit(tok: &str) -> Option<f32> {
    if let Some(p) = tok.strip_suffix('%') {
        return p.trim().parse::<f32>().ok().map(|v| v / 100.0);
    }
    tok.trim().parse::<f32>().ok()
}

/// HSL (h in degrees, s/l in 0..1) → sRGB 8-bit.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (((h / 60.0) % 2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r1, g1, b1) = match (h / 60.0) as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    (
        (((r1 + m) * 255.0).round()).clamp(0.0, 255.0) as u8,
        (((g1 + m) * 255.0).round()).clamp(0.0, 255.0) as u8,
        (((b1 + m) * 255.0).round()).clamp(0.0, 255.0) as u8,
    )
}

/// OKLCH (L 0..1, C chroma, H degrees) → sRGB 8-bit.
fn oklch_to_srgb(l: f32, c: f32, h: f32) -> (u8, u8, u8) {
    let hr = h.to_radians();
    oklab_to_srgb(l, c * hr.cos(), c * hr.sin())
}

/// OKLab (L 0..1, a, b) → sRGB 8-bit. Uses the standard OKLab→linear-sRGB matrices, then the
/// sRGB transfer function, clamped to [0, 255].
fn oklab_to_srgb(l: f32, a: f32, b: f32) -> (u8, u8, u8) {
    // OKLab → LMS' (cube of intermediate).
    let l_ = l + 0.396_337_78 * a + 0.215_803_76 * b;
    let m_ = l - 0.105_561_346 * a - 0.063_854_17 * b;
    let s_ = l - 0.089_484_18 * a - 1.291_485_5 * b;

    let lc = l_ * l_ * l_;
    let mc = m_ * m_ * m_;
    let sc = s_ * s_ * s_;

    // LMS → linear sRGB.
    let lr = 4.076_741_7 * lc - 3.307_711_6 * mc + 0.230_969_94 * sc;
    let lg = -1.268_438 * lc + 2.609_757_4 * mc - 0.341_319_38 * sc;
    let lb = -0.004_196_086 * lc - 0.703_418_6 * mc + 1.707_614_7 * sc;

    (
        srgb_encode(lr),
        srgb_encode(lg),
        srgb_encode(lb),
    )
}

/// Linear sRGB component (0..1, may be out of range) → gamma-encoded 8-bit, clamped.
fn srgb_encode(c: f32) -> u8 {
    let c = c.clamp(0.0, 1.0);
    let v = if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (v * 255.0).round().clamp(0.0, 255.0) as u8
}

fn parse_named_color(lower: &str) -> Option<(u8, u8, u8)> {
    let named = match lower {
        "black" => (0, 0, 0),
        "white" => (255, 255, 255),
        "red" => (255, 0, 0),
        "green" => (0, 128, 0),
        "lime" => (0, 255, 0),
        "blue" => (0, 0, 255),
        "gray" | "grey" => (128, 128, 128),
        "silver" => (192, 192, 192),
        "yellow" => (255, 255, 0),
        "orange" => (255, 165, 0),
        "purple" => (128, 0, 128),
        "cyan" | "aqua" => (0, 255, 255),
        "magenta" | "fuchsia" => (255, 0, 255),
        "maroon" => (128, 0, 0),
        "navy" => (0, 0, 128),
        "teal" => (0, 128, 128),
        "olive" => (128, 128, 0),
        "pink" => (255, 192, 203),
        "brown" => (165, 42, 42),
        _ => return None,
    };
    Some(named)
}

fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.trim();
    let hx = |s: &str| u8::from_str_radix(s, 16).ok();
    match h.len() {
        // #rgb
        3 => {
            let r = hx(&h[0..1])?;
            let g = hx(&h[1..2])?;
            let b = hx(&h[2..3])?;
            Some((r * 17, g * 17, b * 17))
        }
        // #rgba — drop alpha.
        4 => {
            let r = hx(&h[0..1])?;
            let g = hx(&h[1..2])?;
            let b = hx(&h[2..3])?;
            Some((r * 17, g * 17, b * 17))
        }
        // #rrggbb
        6 => {
            let r = hx(&h[0..2])?;
            let g = hx(&h[2..4])?;
            let b = hx(&h[4..6])?;
            Some((r, g, b))
        }
        // #rrggbbaa — drop alpha.
        8 => {
            let r = hx(&h[0..2])?;
            let g = hx(&h[2..4])?;
            let b = hx(&h[4..6])?;
            Some((r, g, b))
        }
        _ => None,
    }
}

/// If any selector in `selectors` matches `el`, return the highest specificity among the
/// matching ones (encoded as id*100 + class*10 + type). `None` if none match.
fn rule_specificity(selectors: &[String], el: &dom::ElementData) -> Option<u32> {
    let mut best: Option<u32> = None;
    for sel in selectors {
        if let Some(spec) = match_simple_selector(sel, el) {
            best = Some(best.map_or(spec, |b| b.max(spec)));
        }
    }
    best
}

/// Match a *simple* selector against an element. Supports a single tag, a single class
/// (`.x`), a single id (`#id`), the universal `*`, and one compound of a tag plus one
/// class/id (e.g. `p.note`, `a#home`). Returns the selector's specificity if it matches.
fn match_simple_selector(sel: &str, el: &dom::ElementData) -> Option<u32> {
    let sel = sel.trim();
    if sel.is_empty() {
        return None;
    }
    if sel == "*" {
        return Some(0);
    }
    // `:root` matches the document root element (the `<html>` element). We approximate by
    // matching any `html` element. Specificity of a pseudo-class is class-level (10).
    if sel.eq_ignore_ascii_case(":root") {
        return if el.tag.eq_ignore_ascii_case("html") { Some(10) } else { None };
    }

    // Split a compound selector into its components: a leading optional type, then a run of
    // `.class` / `#id` parts. We parse character-by-character.
    let mut type_part: Option<String> = None;
    let mut classes: Vec<String> = Vec::new();
    let mut ids: Vec<String> = Vec::new();

    let chars: Vec<char> = sel.chars().collect();
    let mut i = 0;
    // Optional leading type/universal (anything before the first '.' or '#').
    let start = i;
    while i < chars.len() && chars[i] != '.' && chars[i] != '#' {
        i += 1;
    }
    if i > start {
        let t: String = chars[start..i].iter().collect();
        if t == "*" {
            // universal prefix; contributes no specificity, matches any type
        } else if is_ident(&t) {
            type_part = Some(t);
        } else {
            // Unsupported selector syntax (combinators, attributes, pseudo, etc.).
            return None;
        }
    }
    // Remaining `.class` / `#id` parts.
    while i < chars.len() {
        let marker = chars[i];
        if marker != '.' && marker != '#' {
            return None; // unexpected token (e.g. combinator/space) → unsupported
        }
        i += 1;
        let name_start = i;
        while i < chars.len() && chars[i] != '.' && chars[i] != '#' {
            i += 1;
        }
        let name: String = chars[name_start..i].iter().collect();
        if name.is_empty() || !is_ident(&name) {
            return None;
        }
        if marker == '.' {
            classes.push(name);
        } else {
            ids.push(name);
        }
    }

    // Now test the components against the element.
    if let Some(t) = &type_part {
        if !el.tag.eq_ignore_ascii_case(t) {
            return None;
        }
    }
    for id in &ids {
        match el.id() {
            Some(eid) if eid == id => {}
            _ => return None,
        }
    }
    for class in &classes {
        if !el.classes().any(|c| c == class) {
            return None;
        }
    }

    let spec = (ids.len() as u32) * 100
        + (classes.len() as u32) * 10
        + (type_part.is_some() as u32);
    Some(spec)
}

/// A valid CSS identifier for our purposes: letters, digits, `-`, `_`, not starting empty.
fn is_ident(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The built-in user-agent stylesheet: sane defaults for a dark-background renderer.
fn user_agent_stylesheet() -> css::Stylesheet {
    css::parse(
        "html { color: #d8d8d8; font-size: 16px }
         body { color: #d8d8d8; font-size: 16px }
         h1 { font-size: 32px; font-weight: bold; display: block }
         h2 { font-size: 26px; font-weight: bold; display: block }
         h3 { font-size: 20px; font-weight: bold; display: block }
         h4 { font-size: 17px; font-weight: bold; display: block }
         h5 { font-size: 15px; font-weight: bold; display: block }
         h6 { font-size: 13px; font-weight: bold; display: block }
         p { display: block }
         div { display: block }
         section { display: block }
         article { display: block }
         header { display: block }
         footer { display: block }
         nav { display: block }
         main { display: block }
         aside { display: block }
         ul { display: block }
         ol { display: block }
         li { display: block }
         blockquote { display: block }
         pre { display: block }
         table { display: block }
         tr { display: block }
         b { font-weight: bold }
         strong { font-weight: bold }
         i { font-style: italic }
         em { font-style: italic }",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use dom::NodeData;

    fn elem(doc: &dom::Document, tag_and_pred: impl Fn(&dom::ElementData) -> bool) -> dom::NodeId {
        // Find first element matching predicate (depth-first).
        fn walk(
            doc: &dom::Document,
            id: dom::NodeId,
            pred: &dyn Fn(&dom::ElementData) -> bool,
        ) -> Option<dom::NodeId> {
            if let NodeData::Element(e) = &doc.get(id).data {
                if pred(e) {
                    return Some(id);
                }
            }
            for &c in &doc.get(id).children {
                if let Some(found) = walk(doc, c, pred) {
                    return Some(found);
                }
            }
            None
        }
        walk(doc, doc.root(), &tag_and_pred).expect("element not found")
    }

    #[test]
    fn cascade_runs_on_empty_inputs() {
        let doc = dom::Document::new();
        let map = cascade(&doc, &[]);
        assert!(map.is_empty());
    }

    #[test]
    fn ua_defaults_make_h1_big_and_bold() {
        let doc = html::parse("<html><body><h1>Hi</h1></body></html>");
        let map = cascade(&doc, &[]);
        let h1 = elem(&doc, |e| e.tag == "h1");
        let s = &map[&h1];
        assert_eq!(s.font_size, 32.0);
        assert!(s.bold);
        assert!(s.display_block);
    }

    #[test]
    fn id_beats_class_beats_type() {
        let sheet = css::parse(
            "p { color: red } .c { color: green } #x { color: blue }",
        );
        let doc = html::parse(r#"<html><body><p id="x" class="c">t</p></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let p = elem(&doc, |e| e.tag == "p");
        // id selector (#x) wins → blue.
        assert_eq!(map[&p].color, (0, 0, 255));
    }

    #[test]
    fn class_beats_type() {
        let sheet = css::parse("p { color: red } .c { color: green }");
        let doc = html::parse(r#"<html><body><p class="c">t</p></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let p = elem(&doc, |e| e.tag == "p");
        assert_eq!(map[&p].color, (0, 128, 0));
    }

    #[test]
    fn inline_beats_sheet() {
        let sheet = css::parse("#x { color: blue }");
        let doc = html::parse(r#"<html><body><p id="x" style="color: red">t</p></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let p = elem(&doc, |e| e.tag == "p");
        assert_eq!(map[&p].color, (255, 0, 0));
    }

    #[test]
    fn color_and_font_size_inherit_to_children() {
        let sheet = css::parse("#wrap { color: #ff0000; font-size: 24px }");
        let doc = html::parse(
            r#"<html><body><div id="wrap"><span>child</span></div></body></html>"#,
        );
        let map = cascade(&doc, &[sheet]);
        let span = elem(&doc, |e| e.tag == "span");
        assert_eq!(map[&span].color, (255, 0, 0));
        assert_eq!(map[&span].font_size, 24.0);
    }

    #[test]
    fn display_none_propagates_to_subtree() {
        let sheet = css::parse("#h { display: none }");
        let doc = html::parse(
            r#"<html><body><div id="h"><p>hidden</p></div><p>shown</p></body></html>"#,
        );
        let map = cascade(&doc, &[sheet]);
        let hidden_div = elem(&doc, |e| e.id() == Some("h"));
        assert!(map[&hidden_div].display_none);
        // The nested <p> inherits hidden-ness.
        let inner = elem(&doc, |e| {
            e.tag == "p"
                // the hidden one is the first <p>
        });
        // First matching p in doc order is the hidden one.
        assert!(map[&inner].display_none);
    }

    #[test]
    fn compound_selector_matches() {
        let sheet = css::parse("p.note { color: orange }");
        let doc = html::parse(
            r#"<html><body><p class="note">a</p><p>b</p></body></html>"#,
        );
        let map = cascade(&doc, &[sheet]);
        let note = elem(&doc, |e| e.tag == "p" && e.classes().any(|c| c == "note"));
        assert_eq!(map[&note].color, (255, 165, 0));
    }

    #[test]
    fn named_and_hex_colors_parse() {
        assert_eq!(parse_color("#f00"), Some((255, 0, 0)));
        assert_eq!(parse_color("#00ff00"), Some((0, 255, 0)));
        assert_eq!(parse_color("blue"), Some((0, 0, 255)));
        assert_eq!(parse_color("nope"), None);
    }

    #[test]
    fn font_sizes_parse() {
        assert_eq!(parse_font_size("20px", 16.0), Some(20.0));
        assert_eq!(parse_font_size("12pt", 16.0), Some(16.0));
        assert_eq!(parse_font_size("2em", 16.0), Some(32.0));
    }

    #[test]
    fn margin_shorthand_one_value() {
        assert_eq!(parse_edges_shorthand("10px"), Some(Edges::all(10.0)));
    }

    #[test]
    fn margin_shorthand_two_values() {
        // vertical horizontal
        assert_eq!(
            parse_edges_shorthand("10px 20px"),
            Some(Edges { top: 10.0, bottom: 10.0, right: 20.0, left: 20.0 })
        );
    }

    #[test]
    fn margin_shorthand_four_values() {
        // top right bottom left
        assert_eq!(
            parse_edges_shorthand("1px 2px 3px 4px"),
            Some(Edges { top: 1.0, right: 2.0, bottom: 3.0, left: 4.0 })
        );
    }

    #[test]
    fn margin_applied_via_cascade() {
        let sheet = css::parse("p { margin: 5px 10px }");
        let doc = html::parse(r#"<html><body><p>t</p></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let p = elem(&doc, |e| e.tag == "p");
        assert_eq!(map[&p].margin, Edges { top: 5.0, bottom: 5.0, right: 10.0, left: 10.0 });
    }

    #[test]
    fn per_side_override_and_specificity() {
        // The longhand override and a higher-specificity rule both apply on top of shorthand.
        let sheet = css::parse(
            "p { margin: 4px; margin-left: 12px } .x { margin-top: 20px }",
        );
        let doc = html::parse(r#"<html><body><p class="x">t</p></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let p = elem(&doc, |e| e.tag == "p");
        let m = map[&p].margin;
        assert_eq!(m.left, 12.0); // longhand overrode shorthand
        assert_eq!(m.top, 20.0); // higher specificity .x rule wins
        assert_eq!(m.right, 4.0); // untouched shorthand value
        assert_eq!(m.bottom, 4.0);
    }

    #[test]
    fn padding_shorthand_three_values() {
        let sheet = css::parse("div { padding: 1px 2px 3px }");
        let doc = html::parse(r#"<html><body><div>t</div></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let d = elem(&doc, |e| e.tag == "div");
        assert_eq!(
            map[&d].padding,
            Edges { top: 1.0, right: 2.0, left: 2.0, bottom: 3.0 }
        );
    }

    #[test]
    fn border_shorthand_width_and_color() {
        let sheet = css::parse("div { border: 2px solid #ff0000 }");
        let doc = html::parse(r#"<html><body><div>t</div></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let d = elem(&doc, |e| e.tag == "div");
        assert_eq!(map[&d].border, Edges::all(2.0));
        assert_eq!(map[&d].border_color, (255, 0, 0));
    }

    #[test]
    fn border_none_is_zero() {
        let sheet = css::parse("div { border: none }");
        let doc = html::parse(r#"<html><body><div>t</div></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let d = elem(&doc, |e| e.tag == "div");
        assert_eq!(map[&d].border, Edges::all(0.0));
    }

    #[test]
    fn width_parses_to_some() {
        let sheet = css::parse("div { width: 200px; height: auto }");
        let doc = html::parse(r#"<html><body><div>t</div></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let d = elem(&doc, |e| e.tag == "div");
        assert_eq!(map[&d].width, Some(200.0));
        assert_eq!(map[&d].height, None);
    }

    #[test]
    fn percentage_and_garbage_ignored() {
        assert_eq!(parse_length("50%"), None);
        assert_eq!(parse_length("auto"), None);
        assert_eq!(parse_length("garbage"), None);
        assert_eq!(parse_length("12px"), Some(12.0));
        assert_eq!(parse_length("0"), Some(0.0));
    }

    #[test]
    fn display_and_position_parse() {
        let sheet = css::parse(
            "#a { display: flex; position: relative } \
             #b { display: grid } \
             #c { display: inline-block; position: absolute }",
        );
        let doc = html::parse(
            r#"<html><body><div id="a"></div><div id="b"></div><span id="c"></span></body></html>"#,
        );
        let map = cascade(&doc, &[sheet]);
        let a = elem(&doc, |e| e.id() == Some("a"));
        let b = elem(&doc, |e| e.id() == Some("b"));
        let c = elem(&doc, |e| e.id() == Some("c"));
        assert_eq!(map[&a].display, Display::Flex);
        assert_eq!(map[&a].position, Position::Relative);
        assert_eq!(map[&b].display, Display::Grid);
        assert_eq!(map[&c].display, Display::InlineBlock);
        assert_eq!(map[&c].position, Position::Absolute);
    }

    #[test]
    fn display_default_per_tag() {
        let doc = html::parse(r#"<html><body><div></div><span></span></body></html>"#);
        let map = cascade(&doc, &[]);
        let div = elem(&doc, |e| e.tag == "div");
        let span = elem(&doc, |e| e.tag == "span");
        assert_eq!(map[&div].display, Display::Block);
        assert!(map[&div].display_block);
        assert_eq!(map[&span].display, Display::Inline);
        assert!(!map[&span].display_block);
    }

    #[test]
    fn flex_shorthand_expands() {
        assert_eq!(parse_flex_test("1"), (1.0, 1.0, Some(0.0)));
        assert_eq!(parse_flex_test("2 3 40px"), (2.0, 3.0, Some(40.0)));
        assert_eq!(parse_flex_test("none"), (0.0, 0.0, None));
        assert_eq!(parse_flex_test("auto"), (1.0, 1.0, None));
        assert_eq!(parse_flex_test("0 0 100px"), (0.0, 0.0, Some(100.0)));
    }

    fn parse_flex_test(v: &str) -> (f32, f32, Option<f32>) {
        let mut s = ComputedStyle::default();
        apply_flex_shorthand(&mut s, v);
        (s.flex_grow, s.flex_shrink, s.flex_basis)
    }

    #[test]
    fn flex_grow_and_basis_longhand() {
        let sheet = css::parse("#a { flex-grow: 2; flex-basis: 50px }");
        let doc = html::parse(r#"<html><body><div id="a"></div></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let a = elem(&doc, |e| e.id() == Some("a"));
        assert_eq!(map[&a].flex_grow, 2.0);
        assert_eq!(map[&a].flex_basis, Some(50.0));
        assert_eq!(map[&a].flex_shrink, 1.0); // default
    }

    #[test]
    fn gap_one_and_two_values() {
        assert_eq!(parse_gap("10px"), Some((10.0, 10.0)));
        assert_eq!(parse_gap("10px 20px"), Some((10.0, 20.0)));
    }

    #[test]
    fn grid_template_columns_track_parsing() {
        assert_eq!(
            parse_track_list("100px 1fr 50% auto"),
            vec![
                TrackSize::Px(100.0),
                TrackSize::Fr(1.0),
                TrackSize::Pct(50.0),
                TrackSize::Auto
            ]
        );
        // repeat() expansion.
        assert_eq!(
            parse_track_list("repeat(3, 1fr)"),
            vec![TrackSize::Fr(1.0), TrackSize::Fr(1.0), TrackSize::Fr(1.0)]
        );
    }

    #[test]
    fn insets_and_z_index_parse() {
        let sheet =
            css::parse("#a { top: 10px; left: 20px; right: auto; bottom: 5px; z-index: 7 }");
        let doc = html::parse(r#"<html><body><div id="a"></div></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let a = elem(&doc, |e| e.id() == Some("a"));
        assert_eq!(map[&a].top, Some(10.0));
        assert_eq!(map[&a].left, Some(20.0));
        assert_eq!(map[&a].right, None); // auto
        assert_eq!(map[&a].bottom, Some(5.0));
        assert_eq!(map[&a].z_index, Some(7));
    }

    #[test]
    fn grid_placement_parses() {
        assert_eq!(
            parse_grid_placement("1 / 3"),
            Some(GridPlacement { start: Some(1), end: GridEnd::Line(3) })
        );
        assert_eq!(
            parse_grid_placement("2 / span 2"),
            Some(GridPlacement { start: Some(2), end: GridEnd::Span(2) })
        );
        assert_eq!(
            parse_grid_placement("span 3"),
            Some(GridPlacement { start: None, end: GridEnd::Span(3) })
        );
    }

    #[test]
    fn rgb_function_parses() {
        assert_eq!(parse_color("rgb(255 0 0)"), Some((255, 0, 0)));
        assert_eq!(parse_color("rgb(255, 0, 0)"), Some((255, 0, 0)));
        assert_eq!(parse_color("rgba(0, 0, 255, 0.5)"), Some((0, 0, 255)));
        assert_eq!(parse_color("rgb(100% 0% 0%)"), Some((255, 0, 0)));
    }

    #[test]
    fn hsl_function_parses_to_red() {
        let (r, g, b) = parse_color("hsl(0 100% 50%)").unwrap();
        assert!(r > 250, "r={r}");
        assert!(g < 5 && b < 5, "g={g} b={b}");
    }

    #[test]
    fn oklch_red_is_roughly_red() {
        // Tailwind-ish red: high lightness/chroma at ~29deg hue.
        let (r, g, b) = parse_color("oklch(0.628 0.2577 29.23)").unwrap();
        assert!(r > 200, "expected high R, got {r}");
        assert!(g < 120 && b < 120, "expected low-ish G/B, got g={g} b={b}");
        assert!(r > g && r > b, "red should dominate: {r},{g},{b}");
    }

    #[test]
    fn oklab_parses() {
        // Should not panic and stay in range.
        let c = parse_color("oklab(0.5 0.1 0.1)");
        assert!(c.is_some());
    }

    #[test]
    fn hex_alpha_drops_alpha() {
        assert_eq!(parse_color("#ff000080"), Some((255, 0, 0)));
        assert_eq!(parse_color("#f008"), Some((255, 0, 0)));
    }

    #[test]
    fn transparent_yields_no_color() {
        assert_eq!(parse_color("transparent"), None);
    }

    #[test]
    fn var_resolves_from_root_to_descendant() {
        // :root sets --x; a descendant uses color: var(--x).
        let sheet = css::parse(":root { --x: #0000ff } span { color: var(--x) }");
        let doc = html::parse(r#"<html><body><div><span>t</span></div></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let span = elem(&doc, |e| e.tag == "span");
        assert_eq!(map[&span].color, (0, 0, 255));
    }

    #[test]
    fn var_fallback_used_when_undefined() {
        let sheet = css::parse("p { color: var(--missing, #00ff00) }");
        let doc = html::parse(r#"<html><body><p>t</p></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let p = elem(&doc, |e| e.tag == "p");
        assert_eq!(map[&p].color, (0, 255, 0));
    }

    #[test]
    fn var_referencing_var_resolves() {
        let sheet = css::parse(":root { --a: #ff0000; --b: var(--a) } p { color: var(--b) }");
        let doc = html::parse(r#"<html><body><p>t</p></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let p = elem(&doc, |e| e.tag == "p");
        assert_eq!(map[&p].color, (255, 0, 0));
    }

    #[test]
    fn cyclic_var_does_not_hang() {
        let sheet = css::parse(":root { --a: var(--b); --b: var(--a) } p { color: var(--a) }");
        let doc = html::parse(r#"<html><body><p>t</p></body></html>"#);
        // Should terminate (depth cap) and simply not set a color.
        let _ = cascade(&doc, &[sheet]);
    }

    #[test]
    fn current_color_uses_element_color() {
        let sheet = css::parse("p { color: #ff0000; border: 1px solid currentColor }");
        let doc = html::parse(r#"<html><body><p>t</p></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let p = elem(&doc, |e| e.tag == "p");
        assert_eq!(map[&p].border_color, (255, 0, 0));
    }

    #[test]
    fn inherit_keyword_takes_parent_color() {
        let sheet = css::parse("#wrap { color: #ff0000 } span { color: inherit }");
        let doc = html::parse(
            r#"<html><body><div id="wrap"><span>t</span></div></body></html>"#,
        );
        let map = cascade(&doc, &[sheet]);
        let span = elem(&doc, |e| e.tag == "span");
        assert_eq!(map[&span].color, (255, 0, 0));
    }

    #[test]
    fn media_min_width_rule_applies_at_desktop() {
        // min-width:768px applies at the assumed 1280px viewport.
        let sheet = css::parse("@media (min-width: 768px) { p { color: #00ff00 } }");
        let doc = html::parse(r#"<html><body><p>t</p></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let p = elem(&doc, |e| e.tag == "p");
        assert_eq!(map[&p].color, (0, 255, 0));
    }

    #[test]
    fn media_min_width_above_viewport_does_not_apply() {
        let sheet = css::parse(
            "p { color: #ff0000 } @media (min-width: 2000px) { p { color: #00ff00 } }",
        );
        let doc = html::parse(r#"<html><body><p>t</p></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let p = elem(&doc, |e| e.tag == "p");
        // 2000px > 1280px assumed width, so the media rule does not apply: stays red.
        assert_eq!(map[&p].color, (255, 0, 0));
    }

    #[test]
    fn media_max_width_below_viewport_does_not_apply() {
        let sheet = css::parse(
            "p { color: #ff0000 } @media (max-width: 600px) { p { color: #00ff00 } }",
        );
        let doc = html::parse(r#"<html><body><p>t</p></body></html>"#);
        let map = cascade(&doc, &[sheet]);
        let p = elem(&doc, |e| e.tag == "p");
        assert_eq!(map[&p].color, (255, 0, 0));
    }

    #[test]
    fn box_props_do_not_inherit() {
        let sheet = css::parse("#wrap { margin: 30px; padding: 10px; width: 300px }");
        let doc = html::parse(
            r#"<html><body><div id="wrap"><span>child</span></div></body></html>"#,
        );
        let map = cascade(&doc, &[sheet]);
        let span = elem(&doc, |e| e.tag == "span");
        assert_eq!(map[&span].margin, Edges::default());
        assert_eq!(map[&span].padding, Edges::default());
        assert_eq!(map[&span].width, None);
    }
}

