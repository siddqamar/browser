//! Hand-written HTML parsing (Phase 2): a tokenizer plus a forgiving tree builder that
//! populates an arena [`dom::Document`].
//!
//! This is intentionally a pragmatic subset of the HTML5 spec. We do not implement
//! insertion modes, the adoption agency algorithm, or implied tag insertion. The goal is
//! to produce a sensible tree for typical real-world pages and to *never panic* on
//! malformed input.

use std::collections::HashMap;

use dom::{Document, ElementData, NodeData, NodeId};

/// Elements that never have children and need no end tag.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param",
    "source", "track", "wbr",
];

/// Elements whose content is raw text (not parsed as HTML).
const RAWTEXT_ELEMENTS: &[&str] = &["script", "style"];

fn is_void(tag: &str) -> bool {
    VOID_ELEMENTS.contains(&tag)
}

fn is_rawtext(tag: &str) -> bool {
    RAWTEXT_ELEMENTS.contains(&tag)
}

/// Parse an HTML string into a [`dom::Document`].
pub fn parse(html: &str) -> Document {
    // Single source of truth: delegate to the streaming parser. Feeding the whole input then
    // finishing is equivalent to a one-shot parse of the full string.
    let mut p = StreamParser::new();
    p.feed(html.as_bytes());
    p.finish()
}

/// Incremental HTML parser: feed bytes as they arrive, snapshot the partial DOM at any time, and
/// finish to get the complete document.
///
/// Strategy (v1): re-parse the accumulated buffer. Each `feed` appends decoded text to an internal
/// `String`; `snapshot`/`finish` run the existing one-shot [`Parser`] over everything accumulated
/// so far. This guarantees the final DOM is byte-identical to `parse(full_input)` and yields
/// correct progressive snapshots, at the cost of re-parsing. A resumable tokenizer is a future
/// optimization.
#[derive(Default)]
pub struct StreamParser {
    /// All input decoded as valid UTF-8 so far.
    buffer: String,
    /// Trailing bytes from the last `feed` that did not yet form a complete UTF-8 scalar; they
    /// are prepended to the next chunk (or, in `finish`, decoded lossily as end-of-input).
    carry: Vec<u8>,
}

impl StreamParser {
    pub fn new() -> Self {
        StreamParser { buffer: String::new(), carry: Vec::new() }
    }

    /// Append a chunk of raw bytes. Handles a UTF-8 multibyte sequence split across chunk
    /// boundaries by buffering the trailing incomplete bytes until the next `feed`/`finish`.
    pub fn feed(&mut self, chunk: &[u8]) {
        // Combine any carried-over partial sequence with the new chunk.
        let mut bytes = std::mem::take(&mut self.carry);
        bytes.extend_from_slice(chunk);

        match std::str::from_utf8(&bytes) {
            Ok(s) => self.buffer.push_str(s),
            Err(e) => {
                let valid_up_to = e.valid_up_to();
                // Bytes before `valid_up_to` are complete, valid UTF-8.
                // Safe: `valid_up_to` is a valid boundary by definition.
                self.buffer
                    .push_str(unsafe { std::str::from_utf8_unchecked(&bytes[..valid_up_to]) });
                let remainder = &bytes[valid_up_to..];
                // If the trailing error is an incomplete (but possibly valid) sequence, carry it
                // over. Otherwise it is genuinely invalid bytes that completing won't fix, so
                // decode them now (lossily) rather than carry forever.
                if e.error_len().is_none() {
                    self.carry.extend_from_slice(remainder);
                } else {
                    self.buffer
                        .push_str(&String::from_utf8_lossy(remainder));
                }
            }
        }
    }

    /// The current partial DOM — a well-formed, renderable tree of everything parsed so far (open
    /// elements auto-closed by the lenient tree builder so layout/paint can walk it safely).
    pub fn snapshot(&self) -> Document {
        // Decode any carried-over trailing bytes lossily for this point-in-time view, without
        // consuming them from the real carry-over.
        if self.carry.is_empty() {
            Parser::new(&self.buffer).run()
        } else {
            let mut s = self.buffer.clone();
            s.push_str(&String::from_utf8_lossy(&self.carry));
            Parser::new(&s).run()
        }
    }

    /// Consume the parser and return the final complete document (decodes any buffered trailing
    /// bytes as the end of input).
    pub fn finish(mut self) -> Document {
        if !self.carry.is_empty() {
            // End of input: any leftover bytes can no longer be completed; decode lossily.
            let decoded = String::from_utf8_lossy(&self.carry).into_owned();
            self.buffer.push_str(&decoded);
            self.carry.clear();
        }
        Parser::new(&self.buffer).run()
    }
}

struct Parser<'a> {
    input: &'a [u8],
    pos: usize,
    doc: Document,
    /// Stack of currently-open element nodes. The document root is the implicit bottom.
    open: Vec<NodeId>,
    /// Accumulated text characters, flushed to a text node when the next tag begins.
    text_buf: String,
}

impl<'a> Parser<'a> {
    fn new(html: &'a str) -> Self {
        Parser {
            input: html.as_bytes(),
            pos: 0,
            doc: Document::new(),
            open: Vec::new(),
            text_buf: String::new(),
        }
    }

    /// The element we should append new nodes to: the top of the open stack, or the
    /// document root if nothing is open.
    fn current_parent(&self) -> NodeId {
        *self.open.last().unwrap_or(&self.doc.root())
    }

    fn run(mut self) -> Document {
        while self.pos < self.input.len() {
            let c = self.input[self.pos];
            if c == b'<' {
                // Possible tag/comment/doctype. Peek ahead.
                if self.starts_with("<!--") {
                    self.flush_text();
                    self.parse_comment();
                } else if self.starts_with_ci("<!doctype") {
                    self.flush_text();
                    self.parse_doctype();
                } else if self.peek_at(1).map(is_tag_name_start).unwrap_or(false) {
                    self.flush_text();
                    self.parse_start_tag();
                } else if self.peek_at(1) == Some(b'/') {
                    self.flush_text();
                    self.parse_end_tag();
                } else if self.peek_at(1) == Some(b'!') || self.peek_at(1) == Some(b'?') {
                    // Bogus comment / processing instruction: consume to '>'.
                    self.flush_text();
                    self.consume_until_byte(b'>');
                    self.consume_byte(); // the '>'
                } else {
                    // A stray '<' that doesn't begin a markup construct: literal text.
                    self.text_buf.push('<');
                    self.pos += 1;
                }
            } else {
                // Text. Accumulate decoded characters until the next '<'.
                self.consume_text_until_lt();
            }
        }
        self.flush_text();
        self.doc
    }

    // ---- low-level cursor helpers ----

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.input.get(self.pos + offset).copied()
    }

    fn consume_byte(&mut self) {
        if self.pos < self.input.len() {
            self.pos += 1;
        }
    }

    fn starts_with(&self, s: &str) -> bool {
        self.input[self.pos..].starts_with(s.as_bytes())
    }

    /// Case-insensitive `starts_with` for ASCII.
    fn starts_with_ci(&self, s: &str) -> bool {
        let bytes = s.as_bytes();
        if self.pos + bytes.len() > self.input.len() {
            return false;
        }
        self.input[self.pos..self.pos + bytes.len()]
            .iter()
            .zip(bytes)
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
    }

    fn consume_until_byte(&mut self, byte: u8) {
        while let Some(c) = self.peek_at(0) {
            if c == byte {
                break;
            }
            self.pos += 1;
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek_at(0) {
            if c.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    // ---- text ----

    /// Consume a run of text (not raw text), decoding character references, until the next
    /// `<` or end of input.
    fn consume_text_until_lt(&mut self) {
        while let Some(c) = self.peek_at(0) {
            if c == b'<' {
                break;
            }
            if c == b'&' {
                self.consume_entity_into_text();
            } else {
                // Copy one UTF-8 scalar. We operate on bytes but the input came from a
                // &str, so byte boundaries that are not '<'/'&' are safe to copy verbatim.
                let start = self.pos;
                self.pos += 1;
                // Extend over continuation bytes of a multi-byte UTF-8 sequence.
                while let Some(b) = self.peek_at(0) {
                    if b & 0b1100_0000 == 0b1000_0000 {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                // Safe: bytes [start, self.pos) form complete UTF-8 from the original &str.
                let s = std::str::from_utf8(&self.input[start..self.pos]).unwrap_or("");
                self.text_buf.push_str(s);
            }
        }
    }

    /// Decode a character reference starting at the current `&` into `text_buf`. On any
    /// malformed reference, the literal `&` (and consumed chars) pass through.
    fn consume_entity_into_text(&mut self) {
        let decoded = self.read_entity();
        self.text_buf.push_str(&decoded);
    }

    /// Read a character reference at the cursor (which is on `&`). Advances the cursor and
    /// returns the decoded string, or the literal text if it is not a valid reference.
    fn read_entity(&mut self) -> String {
        debug_assert_eq!(self.peek_at(0), Some(b'&'));
        let amp_start = self.pos;
        self.pos += 1; // consume '&'

        if self.peek_at(0) == Some(b'#') {
            self.pos += 1; // consume '#'
            let hex = matches!(self.peek_at(0), Some(b'x') | Some(b'X'));
            if hex {
                self.pos += 1;
            }
            let digits_start = self.pos;
            while let Some(c) = self.peek_at(0) {
                let ok = if hex { c.is_ascii_hexdigit() } else { c.is_ascii_digit() };
                if ok {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.pos == digits_start {
                // No digits: not a valid numeric reference.
                return self.literal_from(amp_start);
            }
            let digits = std::str::from_utf8(&self.input[digits_start..self.pos]).unwrap_or("");
            let code = u32::from_str_radix(digits, if hex { 16 } else { 10 }).ok();
            // Optional trailing semicolon.
            if self.peek_at(0) == Some(b';') {
                self.pos += 1;
            }
            return match code.and_then(char::from_u32) {
                Some(ch) => ch.to_string(),
                None => '\u{FFFD}'.to_string(),
            };
        }

        // Named reference: read ASCII alphanumerics.
        let name_start = self.pos;
        while let Some(c) = self.peek_at(0) {
            if c.is_ascii_alphanumeric() {
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == name_start {
            return self.literal_from(amp_start);
        }
        let name = std::str::from_utf8(&self.input[name_start..self.pos]).unwrap_or("");
        if let Some(replacement) = named_entity(name) {
            // Consume an optional trailing semicolon.
            if self.peek_at(0) == Some(b';') {
                self.pos += 1;
            }
            replacement.to_string()
        } else {
            // Unknown entity: pass through literally (the '&' + name we consumed).
            self.literal_from(amp_start)
        }
    }

    /// Return the literal input bytes from `start` to the current cursor as a string.
    fn literal_from(&self, start: usize) -> String {
        std::str::from_utf8(&self.input[start..self.pos])
            .unwrap_or("")
            .to_string()
    }

    fn flush_text(&mut self) {
        if self.text_buf.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.text_buf);
        let parent = self.current_parent();
        self.doc.append_child(parent, NodeData::Text(text));
    }

    // ---- comments / doctype ----

    fn parse_comment(&mut self) {
        // cursor on "<!--"
        self.pos += 4;
        let start = self.pos;
        let end = loop {
            if self.pos >= self.input.len() {
                break self.input.len();
            }
            if self.input[self.pos..].starts_with(b"-->") {
                break self.pos;
            }
            self.pos += 1;
        };
        let content = std::str::from_utf8(&self.input[start..end]).unwrap_or("").to_string();
        if self.pos < self.input.len() {
            self.pos += 3; // consume "-->"
        }
        let parent = self.current_parent();
        self.doc.append_child(parent, NodeData::Comment(content));
    }

    fn parse_doctype(&mut self) {
        // Consume up to and including '>'. We ignore the doctype entirely.
        self.consume_until_byte(b'>');
        self.consume_byte();
    }

    // ---- tags ----

    fn parse_start_tag(&mut self) {
        // cursor on '<'
        self.pos += 1;
        let tag = self.read_tag_name();
        if tag.is_empty() {
            // Shouldn't happen given the caller's check, but be defensive.
            return;
        }
        let mut attrs: HashMap<String, String> = HashMap::new();
        let mut self_closing = false;

        loop {
            self.skip_whitespace();
            match self.peek_at(0) {
                None => break,
                Some(b'>') => {
                    self.pos += 1;
                    break;
                }
                Some(b'/') => {
                    self.pos += 1;
                    if self.peek_at(0) == Some(b'>') {
                        self_closing = true;
                        self.pos += 1;
                        break;
                    }
                    // stray '/', keep going
                }
                _ => {
                    let (name, value) = self.read_attribute();
                    if !name.is_empty() {
                        attrs.entry(name).or_insert(value);
                    } else {
                        // No progress would loop forever; force advance.
                        self.consume_byte();
                    }
                }
            }
        }

        let parent = self.current_parent();
        let node = self.doc.append_child(
            parent,
            NodeData::Element(ElementData { tag: tag.clone(), attrs }),
        );

        if is_void(&tag) || self_closing {
            // Void / self-closing elements don't open a scope.
            return;
        }

        if is_rawtext(&tag) {
            // Raw-text element: consume everything up to the matching end tag as one text
            // node and do not push onto the open stack.
            self.consume_rawtext(&tag, node);
            return;
        }

        self.open.push(node);
    }

    /// Consume raw text up to (but not parsing) `</tag>`, appending it as a single text
    /// child of `node`.
    fn consume_rawtext(&mut self, tag: &str, node: NodeId) {
        let close = format!("</{tag}");
        let start = self.pos;
        let end = loop {
            if self.pos >= self.input.len() {
                break self.input.len();
            }
            if self.starts_with_ci(&close) {
                break self.pos;
            }
            self.pos += 1;
        };
        let content = std::str::from_utf8(&self.input[start..end]).unwrap_or("").to_string();
        if !content.is_empty() {
            self.doc.append_child(node, NodeData::Text(content));
        }
        // Consume the end tag if present.
        if self.pos < self.input.len() {
            // skip "</tag" then up to and including '>'
            self.pos += close.len();
            self.consume_until_byte(b'>');
            self.consume_byte();
        }
    }

    fn parse_end_tag(&mut self) {
        // cursor on '<', next is '/'
        self.pos += 2;
        let tag = self.read_tag_name();
        // Consume to '>'.
        self.consume_until_byte(b'>');
        self.consume_byte();
        if tag.is_empty() {
            return;
        }
        // Find nearest matching open element.
        if let Some(idx) = self.open.iter().rposition(|&id| {
            matches!(&self.doc.get(id).data, NodeData::Element(e) if e.tag == tag)
        }) {
            // Pop everything above it, plus the match itself.
            self.open.truncate(idx);
        }
        // If there's no match, ignore the stray end tag.
    }

    /// Read a lowercased tag name at the cursor.
    fn read_tag_name(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek_at(0) {
            if is_tag_name_char(c) {
                self.pos += 1;
            } else {
                break;
            }
        }
        std::str::from_utf8(&self.input[start..self.pos])
            .unwrap_or("")
            .to_ascii_lowercase()
    }

    /// Read one attribute (name plus optional value) at the cursor.
    fn read_attribute(&mut self) -> (String, String) {
        // Read name.
        let name_start = self.pos;
        while let Some(c) = self.peek_at(0) {
            if c.is_ascii_whitespace() || c == b'=' || c == b'>' || c == b'/' {
                break;
            }
            self.pos += 1;
        }
        let name = std::str::from_utf8(&self.input[name_start..self.pos])
            .unwrap_or("")
            .to_ascii_lowercase();
        if name.is_empty() {
            return (String::new(), String::new());
        }

        self.skip_whitespace();
        if self.peek_at(0) != Some(b'=') {
            // Boolean attribute.
            return (name, String::new());
        }
        self.pos += 1; // consume '='
        self.skip_whitespace();

        let value = match self.peek_at(0) {
            Some(q @ (b'"' | b'\'')) => {
                self.pos += 1; // opening quote
                let v = self.read_attr_value_until(|c| c == q);
                if self.peek_at(0) == Some(q) {
                    self.pos += 1; // closing quote
                }
                v
            }
            _ => self.read_attr_value_until(|c| c.is_ascii_whitespace() || c == b'>'),
        };
        (name, value)
    }

    /// Read an attribute value with entity decoding, stopping when `stop` matches (the
    /// stop byte is not consumed).
    fn read_attr_value_until(&mut self, stop: impl Fn(u8) -> bool) -> String {
        let mut out = String::new();
        while let Some(c) = self.peek_at(0) {
            if stop(c) {
                break;
            }
            if c == b'&' {
                out.push_str(&self.read_entity());
            } else {
                let start = self.pos;
                self.pos += 1;
                while let Some(b) = self.peek_at(0) {
                    if b & 0b1100_0000 == 0b1000_0000 {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                out.push_str(std::str::from_utf8(&self.input[start..self.pos]).unwrap_or(""));
            }
        }
        out
    }
}

fn is_tag_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic()
}

fn is_tag_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'-' || c == b':' || c == b'_'
}

/// Decode a named character reference. Returns `None` for unknown names.
fn named_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{00A0}",
        "copy" => "\u{00A9}",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: collect child node ids of `id`.
    fn children(doc: &Document, id: NodeId) -> Vec<NodeId> {
        doc.get(id).children.clone()
    }

    fn tag_of(doc: &Document, id: NodeId) -> &str {
        match &doc.get(id).data {
            NodeData::Element(e) => &e.tag,
            _ => "",
        }
    }

    fn text_of(doc: &Document, id: NodeId) -> Option<&str> {
        match &doc.get(id).data {
            NodeData::Text(t) => Some(t),
            _ => None,
        }
    }

    #[test]
    fn empty_parse_has_root_node_zero() {
        let doc = parse("");
        assert_eq!(doc.root(), dom::NodeId(0));
    }

    #[test]
    fn nests_p_under_body_under_html() {
        let doc = parse("<html><body><p>hi</p></body></html>");
        let root = doc.root();
        let html = children(&doc, root)[0];
        assert_eq!(tag_of(&doc, html), "html");
        let body = children(&doc, html)[0];
        assert_eq!(tag_of(&doc, body), "body");
        let p = children(&doc, body)[0];
        assert_eq!(tag_of(&doc, p), "p");
        let text = children(&doc, p)[0];
        assert_eq!(text_of(&doc, text), Some("hi"));
    }

    #[test]
    fn parses_attributes_quoted_unquoted_boolean() {
        let doc = parse(r#"<input type="text" name='n' size=10 disabled>"#);
        let input = children(&doc, doc.root())[0];
        let attrs = match &doc.get(input).data {
            NodeData::Element(e) => &e.attrs,
            _ => panic!("expected element"),
        };
        assert_eq!(attrs.get("type").map(String::as_str), Some("text"));
        assert_eq!(attrs.get("name").map(String::as_str), Some("n"));
        assert_eq!(attrs.get("size").map(String::as_str), Some("10"));
        assert_eq!(attrs.get("disabled").map(String::as_str), Some(""));
    }

    #[test]
    fn void_element_has_no_children_and_siblings_attach() {
        let doc = parse("<div><img src=x>after</div>");
        let div = children(&doc, doc.root())[0];
        let kids = children(&doc, div);
        assert_eq!(tag_of(&doc, kids[0]), "img");
        assert!(children(&doc, kids[0]).is_empty());
        // The text "after" is a sibling of img under div, not a child of img.
        assert_eq!(text_of(&doc, kids[1]), Some("after"));
    }

    #[test]
    fn decodes_entities_in_text() {
        let doc = parse("<p>a &amp; b &lt;c&gt; &#39;x&#39; &#x41; &nbsp;&copy;</p>");
        let p = children(&doc, doc.root())[0];
        let t = children(&doc, p)[0];
        assert_eq!(text_of(&doc, t), Some("a & b <c> 'x' A \u{00A0}\u{00A9}"));
    }

    #[test]
    fn unknown_entity_passes_through() {
        let doc = parse("<p>5 &notreal; x</p>");
        let p = children(&doc, doc.root())[0];
        let t = children(&doc, p)[0];
        assert_eq!(text_of(&doc, t), Some("5 &notreal; x"));
    }

    #[test]
    fn creates_comment_node() {
        let doc = parse("<div><!-- hello --></div>");
        let div = children(&doc, doc.root())[0];
        let c = children(&doc, div)[0];
        match &doc.get(c).data {
            NodeData::Comment(s) => assert_eq!(s, " hello "),
            other => panic!("expected comment, got {other:?}"),
        }
    }

    #[test]
    fn script_rawtext_is_not_parsed() {
        let doc = parse("<script>if (a<b) { x = 0; }</script>");
        let script = children(&doc, doc.root())[0];
        assert_eq!(tag_of(&doc, script), "script");
        let kids = children(&doc, script);
        assert_eq!(kids.len(), 1);
        assert_eq!(text_of(&doc, kids[0]), Some("if (a<b) { x = 0; }"));
        // No bogus <b> element anywhere.
        for n in 0..doc.len() {
            assert_ne!(tag_of(&doc, NodeId(n)), "b");
        }
    }

    #[test]
    fn style_rawtext_is_not_parsed() {
        let doc = parse("<style>a > b { color: red }</style>");
        let style = children(&doc, doc.root())[0];
        assert_eq!(tag_of(&doc, style), "style");
        let kids = children(&doc, style);
        assert_eq!(text_of(&doc, kids[0]), Some("a > b { color: red }"));
    }

    #[test]
    fn doctype_is_ignored() {
        let doc = parse("<!DOCTYPE html><html></html>");
        let kids = children(&doc, doc.root());
        // Only the html element, no doctype/text node.
        assert_eq!(kids.len(), 1);
        assert_eq!(tag_of(&doc, kids[0]), "html");
    }

    #[test]
    fn self_closing_tag() {
        let doc = parse("<div><br/>x</div>");
        let div = children(&doc, doc.root())[0];
        let kids = children(&doc, div);
        assert_eq!(tag_of(&doc, kids[0]), "br");
        assert!(children(&doc, kids[0]).is_empty());
        assert_eq!(text_of(&doc, kids[1]), Some("x"));
    }

    #[test]
    fn mismatched_end_tag_pops_to_ancestor() {
        // </b> has no match; should be ignored. </span> closes span.
        let doc = parse("<span>a</b>b</span>c");
        let root = doc.root();
        let span = children(&doc, root)[0];
        assert_eq!(tag_of(&doc, span), "span");
        // span has two text children "a" and "b".
        let kids = children(&doc, span);
        assert_eq!(text_of(&doc, kids[0]), Some("a"));
        assert_eq!(text_of(&doc, kids[1]), Some("b"));
        // "c" is a sibling of span under root.
        let root_kids = children(&doc, root);
        assert_eq!(text_of(&doc, root_kids[1]), Some("c"));
    }

    #[test]
    fn unbalanced_input_does_not_panic() {
        // A grab-bag of malformed constructs.
        let _ = parse("<<<>>> <div class=>< <!-- unterminated <p attr");
        let _ = parse("</></p></div>< & &# &#x &;");
        let _ = parse("<script>unterminated");
        let _ = parse("<!DOCTYPE");
        let _ = parse("<a href=\"unclosed quote>text");
        // If we got here without panicking, the test passes.
    }

    #[test]
    fn tag_names_are_lowercased() {
        let doc = parse("<DIV><SPAN>x</SPAN></DIV>");
        let div = children(&doc, doc.root())[0];
        assert_eq!(tag_of(&doc, div), "div");
        let span = children(&doc, div)[0];
        assert_eq!(tag_of(&doc, span), "span");
    }

    // ---- StreamParser tests ----

    /// Total node count, used to compare tree structure between parses.
    fn node_count(doc: &Document) -> usize {
        doc.len()
    }

    #[test]
    fn stream_feed_in_two_halves_equals_one_shot() {
        let whole = "<html><body><p>Hello, world</p><div class=\"x\">more</div></body></html>";
        // Split mid-tag: the first half ends inside `<div`.
        let split = whole.find("class").unwrap() - 4; // somewhere inside the <div tag
        let (a, b) = whole.split_at(split);

        let mut p = StreamParser::new();
        p.feed(a.as_bytes());
        p.feed(b.as_bytes());
        let streamed = p.finish();

        let one_shot = parse(whole);
        assert_eq!(node_count(&streamed), node_count(&one_shot));

        // Structural spot-check: html > body > p with text "Hello, world".
        let html = children(&streamed, streamed.root())[0];
        assert_eq!(tag_of(&streamed, html), "html");
        let body = children(&streamed, html)[0];
        let para = children(&streamed, body)[0];
        let text = children(&streamed, para)[0];
        assert_eq!(text_of(&streamed, text), Some("Hello, world"));
    }

    #[test]
    fn stream_multibyte_utf8_split_across_feeds() {
        let whole = "<p>café 🎉</p>";
        let bytes = whole.as_bytes();
        // Find a split point that lands in the middle of the emoji's 4-byte sequence.
        let emoji_start = whole.find('🎉').unwrap();
        let split = emoji_start + 2; // mid-emoji byte boundary

        let mut p = StreamParser::new();
        p.feed(&bytes[..split]);
        p.feed(&bytes[split..]);
        let doc = p.finish();

        let para = children(&doc, doc.root())[0];
        let text = children(&doc, para)[0];
        assert_eq!(text_of(&doc, text), Some("café 🎉"));
    }

    #[test]
    fn stream_snapshot_on_truncated_prefix_does_not_panic() {
        let mut p = StreamParser::new();
        p.feed(b"<html><body><p>Hello");
        let doc = p.snapshot();

        // The lenient tree builder auto-closes open elements; the partial text is present.
        let html = children(&doc, doc.root())[0];
        let body = children(&doc, html)[0];
        let para = children(&doc, body)[0];
        let text = children(&doc, para)[0];
        assert_eq!(text_of(&doc, text), Some("Hello"));

        // Snapshotting again with a truncated open tag must also not panic.
        p.feed(b"<div class=\"");
        let _ = p.snapshot();
    }

    #[test]
    fn stream_regression_matches_parse() {
        let cases = [
            "<html><body><p>hi</p></body></html>",
            r#"<input type="text" name='n' size=10 disabled>"#,
            "<div><img src=x>after</div>",
            "<script>if (a<b) { x = 0; }</script>",
            "<span>a</b>b</span>c",
        ];
        for case in cases {
            let mut p = StreamParser::new();
            p.feed(case.as_bytes());
            let streamed = p.finish();
            let one_shot = parse(case);
            assert_eq!(
                node_count(&streamed),
                node_count(&one_shot),
                "node count mismatch for {case:?}"
            );
        }
    }
}
