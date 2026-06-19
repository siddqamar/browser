//! WebDriver server binary. Usage: `cargo run -p webdriver -- --port 4444`.

fn main() {
    let mut port: u16 = 4444;
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--port" | "-p" => {
                if let Some(v) = args.get(i + 1).and_then(|s| s.parse().ok()) {
                    port = v;
                }
                i += 2;
            }
            "--help" | "-h" => {
                eprintln!("usage: webdriver [--port <port>]   (default 4444)");
                return;
            }
            _ => i += 1,
        }
    }

    if let Err(e) = webdriver::server::run(port) {
        eprintln!("webdriver server error: {e}");
        std::process::exit(1);
    }
}
