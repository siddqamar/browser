//! The `backend-lumen` implementation of the JS runtime: the from-scratch [`lumen`] engine wired to
//! the same [`crate::EvalOutput`] surface the rest of the codebase consumes. Selected at compile
//! time via `--features backend-lumen` (no V8 is linked).
//!
//! Scope: the language-evaluation slice — [`Runtime`] (persistent realm + `eval`) and
//! [`eval_batch`]. The DOM-aware paths (`run_with_dom`, `run_modules`, `Session`) are still V8-only
//! and are simply not present under this feature; the browser engine keeps the default `backend-v8`
//! until lumen grows DOM bindings.

use crate::EvalOutput;
use lumen::{Completion, Engine};

/// A JS runtime backed by lumen. Mirrors the `backend-v8` `Runtime`: one engine instance whose
/// global state persists across [`eval`](Runtime::eval) calls.
pub struct Runtime {
    engine: Engine,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Runtime { engine: Engine::new() }
    }

    /// Evaluate a script in the persistent realm. Never panics on a JS error — it is captured into
    /// [`EvalOutput::error`], matching the `backend-v8` contract.
    pub fn eval(&mut self, source: &str) -> EvalOutput {
        let result = self.engine.eval(source, false);
        let console = self.engine.take_console();
        match result {
            Ok(Completion::Value(v)) => EvalOutput {
                value: if v.is_empty() { None } else { Some(v) },
                console,
                error: None,
            },
            Ok(Completion::Throw { name, message }) => EvalOutput {
                value: None,
                console,
                error: Some(format_throw(&name, &message)),
            },
            Err(e) => EvalOutput {
                value: None,
                console,
                error: Some(format!("SyntaxError: {} (line {})", e.message, e.line)),
            },
        }
    }
}

/// Run `sources` in order on a single fresh runtime (so later scripts see earlier globals) and
/// return one [`EvalOutput`] per source. The `backend-lumen` analogue of the V8 `eval_batch`; here
/// it is a plain in-thread loop (lumen needs no isolate-per-thread dance).
pub fn eval_batch(sources: Vec<String>) -> Vec<EvalOutput> {
    let mut rt = Runtime::new();
    sources.iter().map(|s| rt.eval(s)).collect()
}

fn format_throw(name: &str, message: &str) -> String {
    match (name.is_empty(), message.is_empty()) {
        (true, _) => format!("Uncaught {message}"),
        (false, true) => format!("Uncaught {name}"),
        (false, false) => format!("Uncaught {name}: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arithmetic_returns_value_string() {
        let mut rt = Runtime::new();
        let out = rt.eval("1 + 2 * 3");
        assert_eq!(out.value.as_deref(), Some("7"));
        assert!(out.error.is_none());
    }

    #[test]
    fn error_is_captured_not_panicked() {
        let mut rt = Runtime::new();
        let out = rt.eval("null.x");
        assert!(out.value.is_none());
        assert!(out.error.as_deref().unwrap().contains("TypeError"));
    }

    #[test]
    fn state_persists_across_eval() {
        let mut rt = Runtime::new();
        rt.eval("var counter = 0;");
        rt.eval("counter += 5;");
        assert_eq!(rt.eval("counter").value.as_deref(), Some("5"));
    }
}
