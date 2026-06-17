//! Diagnostic: measure reveal (display:none before/after JS) + remaining errors.
//! Usage: cargo run -p engine --example dump -- <file.html> <base-url>

fn none_count(doc: &dom::Document, base: &str) -> (usize, usize) {
    let (sheets, _n) = engine::collect_stylesheets(doc, base);
    let computed = style::cascade(doc, &sheets);
    (computed.len(), computed.values().filter(|c| c.display_none).count())
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: dump <file.html> <base>");
    let base = std::env::args().nth(2).unwrap_or_else(|| "https://example.com/".into());
    let bytes = std::fs::read(&path).expect("read file");
    let html = String::from_utf8_lossy(&bytes);

    let doc = html::parse(&html);
    let (n, none_before) = none_count(&doc, &base);
    println!("BEFORE: {n} elements, {none_before} display:none");

    let (doc, console) = engine::run_scripts(doc, &base);
    let errs: Vec<&String> = console.iter().filter(|l| l.starts_with('\u{26a0}')).collect();
    let (_n, none_after) = none_count(&doc, &base);
    println!("AFTER:  {none_after} display:none  (revealed {})", none_before as i64 - none_after as i64);
    println!("errors: {}", errs.len());
    for e in &errs {
        println!("{}", e.lines().next().unwrap_or(""));
        for l in e.lines().skip(1).take(2) {
            println!("{l}");
        }
    }
}
