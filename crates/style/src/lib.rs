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
    cascade_node(doc, doc.root(), &initial, false, &ua, sheets, &mut out);
    out
}

/// Recursively compute styles. `parent` is the parent's computed style (the inheritance
/// source); `parent_hidden` is true if any ancestor was `display: none`.
#[allow(clippy::too_many_arguments)]
fn cascade_node(
    doc: &dom::Document,
    id: dom::NodeId,
    parent: &ComputedStyle,
    parent_hidden: bool,
    ua: &css::Stylesheet,
    author: &[css::Stylesheet],
    out: &mut HashMap<dom::NodeId, ComputedStyle>,
) {
    let node = doc.get(id);
    let computed = if let dom::NodeData::Element(el) = &node.data {
        let style = compute_element_style(el, parent, parent_hidden, ua, author);
        out.insert(id, style.clone());
        style
    } else {
        // Non-elements inherit the parent style so text runs can read color/size off the
        // nearest element ancestor via the parent passed down.
        parent.clone()
    };
    let hidden = parent_hidden || computed.display_none;
    for &child in &node.children {
        cascade_node(doc, child, &computed, hidden, ua, author, out);
    }
}

/// Resolve one element's computed style: gather matching declarations from all origins in
/// precedence order, apply them, then layer inheritance.
fn compute_element_style(
    el: &dom::ElementData,
    parent: &ComputedStyle,
    parent_hidden: bool,
    ua: &css::Stylesheet,
    author: &[css::Stylesheet],
) -> ComputedStyle {
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
        if let Some(spec) = rule_specificity(&rule.selectors, el) {
            matches.push(MatchEntry { origin: 0, specificity: spec, order, decls: &rule.declarations });
        }
        order += 1;
    }
    for sheet in author {
        for rule in &sheet.rules {
            if let Some(spec) = rule_specificity(&rule.selectors, el) {
                matches.push(MatchEntry { origin: 1, specificity: spec, order, decls: &rule.declarations });
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

    for m in &matches {
        for (prop, val) in m.decls {
            apply_declaration(&mut style, prop, val, parent);
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

    style
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
fn apply_declaration(style: &mut ComputedStyle, prop: &str, val: &str, parent: &ComputedStyle) {
    match prop {
        "color" => {
            if let Some(c) = parse_color(val) {
                style.color = c;
            }
        }
        "background-color" | "background" => {
            if let Some(c) = parse_color(val) {
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
        "border" => apply_border_shorthand(style, val, EdgeSide::All),
        "border-top" => apply_border_shorthand(style, val, EdgeSide::Top),
        "border-right" => apply_border_shorthand(style, val, EdgeSide::Right),
        "border-bottom" => apply_border_shorthand(style, val, EdgeSide::Bottom),
        "border-left" => apply_border_shorthand(style, val, EdgeSide::Left),
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
            if let Some(c) = parse_color(val) {
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
fn apply_border_shorthand(style: &mut ComputedStyle, val: &str, side: EdgeSide) {
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
            if let Some(c) = parse_color(tok) {
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

/// Parse a color: `#rgb`, `#rrggbb`, or a small set of named colors. `None` if unrecognized.
fn parse_color(val: &str) -> Option<(u8, u8, u8)> {
    let v = val.trim();
    if let Some(hex) = v.strip_prefix('#') {
        return parse_hex(hex);
    }
    let named = match v.to_ascii_lowercase().as_str() {
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
    match h.len() {
        3 => {
            let r = u8::from_str_radix(&h[0..1], 16).ok()?;
            let g = u8::from_str_radix(&h[1..2], 16).ok()?;
            let b = u8::from_str_radix(&h[2..3], 16).ok()?;
            Some((r * 17, g * 17, b * 17))
        }
        6 => {
            let r = u8::from_str_radix(&h[0..2], 16).ok()?;
            let g = u8::from_str_radix(&h[2..4], 16).ok()?;
            let b = u8::from_str_radix(&h[4..6], 16).ok()?;
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
