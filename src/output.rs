//! Output contract: JSON to stdout, pretty iff TTY and not forced compact.
//! Errors as `{"error": "..."}` to stderr. `--fields` filtering with dotted paths.

use std::io::{IsTerminal, Write};

use anyhow::Result;
use serde_json::{Map, Value};

/// Holds output-related flags from the global CLI.
#[derive(Debug, Clone, Default)]
pub struct Output {
    /// When true, always emit compact JSON regardless of TTY state.
    /// Set by `--json`.
    pub force_compact: bool,
    /// Optional comma-split list of field paths (already split by the caller).
    /// `None` means no filtering. Each entry may contain dots for nesting.
    pub fields: Option<Vec<String>>,
}

/// Returns true iff stdout is a terminal.
pub fn is_stdout_tty() -> bool {
    std::io::stdout().is_terminal()
}

/// Serialize `value` (after optional field filtering) to stdout.
///
/// Pretty-prints iff stdout is a TTY AND `out.force_compact == false`.
/// Always prints a trailing newline and flushes.
pub fn emit(out: &Output, value: Value) -> Result<()> {
    let filtered = match &out.fields {
        Some(fields) if !fields.is_empty() => filter_fields(value, fields),
        _ => value,
    };

    let pretty = is_stdout_tty() && !out.force_compact;
    let serialized = if pretty {
        serde_json::to_string_pretty(&filtered)?
    } else {
        serde_json::to_string(&filtered)?
    };

    let mut stdout = std::io::stdout().lock();
    stdout.write_all(serialized.as_bytes())?;
    stdout.write_all(b"\n")?;
    stdout.flush()?;
    Ok(())
}

/// Emit a compact `{"error": "<msg>"}` JSON object to stderr + newline.
/// Exit-code handling is the caller's (main.rs) responsibility.
pub fn emit_error(msg: &str) {
    let payload = serde_json::json!({ "error": msg });
    // Best-effort: if stderr write fails there is nothing sensible to do.
    let serialized =
        serde_json::to_string(&payload).unwrap_or_else(|_| "{\"error\":\"unknown\"}".to_string());
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(serialized.as_bytes());
    let _ = stderr.write_all(b"\n");
    let _ = stderr.flush();
}

/// A node in the selection tree. Empty map means "keep this subtree entirely".
type SelectionTree = Map<String, Value>;

/// Parse a list of dotted field paths into a nested selection tree.
///
/// `["id", "foodName", "energy.quantity", "energy.unit"]` becomes
/// `{id:{}, foodName:{}, energy:{quantity:{}, unit:{}}}`.
fn build_selection_tree(fields: &[String]) -> SelectionTree {
    let mut root: SelectionTree = Map::new();
    for path in fields {
        // Skip empty specs (e.g. trailing comma).
        let trimmed = path.trim();
        if trimmed.is_empty() {
            continue;
        }
        insert_path(&mut root, trimmed);
    }
    root
}

fn insert_path(node: &mut SelectionTree, path: &str) {
    let mut parts = path.splitn(2, '.');
    let head = match parts.next() {
        Some(h) if !h.is_empty() => h,
        _ => return,
    };
    let tail = parts.next();

    match tail {
        None => {
            // Leaf: request the entire subtree. If a more specific selection
            // was already inserted (e.g. `energy.quantity` then `energy`),
            // collapse to "keep everything" by overwriting with an empty map.
            // If a leaf was already present, leave it.
            node.insert(head.to_string(), Value::Object(Map::new()));
        }
        Some(rest) => {
            // If the child already exists as a "keep everything" marker (an
            // earlier leaf insertion for this same key), the broader
            // selection subsumes this narrower one.
            if let Some(existing) = node.get(head) {
                if matches!(existing, Value::Object(m) if m.is_empty()) {
                    // But only if the prior insertion was a LEAF (no dotted
                    // continuation). A freshly-created empty map from a
                    // previous nested insert wouldn't stay empty — it would
                    // have its child inserted. Distinguish by tracking
                    // whether the key was inserted as a leaf.
                    //
                    // Our ordering invariant: an empty map at `head` can
                    // only arise from a prior leaf insert, because nested
                    // inserts immediately populate the child below.
                    return;
                }
            }

            // Ensure the child exists as an object we can recurse into.
            let entry = node
                .entry(head.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(child) = entry {
                insert_path(child, rest);
            }
        }
    }
}

/// Apply the field filter tree to `value`.
///
/// Public entry point: splits the flat field list into a selection tree and
/// walks `value`.
pub fn filter_fields(value: Value, fields: &[String]) -> Value {
    let tree = build_selection_tree(fields);
    if tree.is_empty() {
        return value;
    }
    apply_tree(value, &tree)
}

fn apply_tree(value: Value, tree: &SelectionTree) -> Value {
    // An empty tree means "keep everything".
    if tree.is_empty() {
        return value;
    }

    match value {
        Value::Object(map) => {
            let mut out = Map::with_capacity(map.len().min(tree.len()));
            // Preserve the source object's key order.
            for (k, v) in map {
                if let Some(sub) = tree.get(&k) {
                    let sub_tree = match sub {
                        Value::Object(m) => m,
                        // Defensive: build_selection_tree only inserts Object values.
                        _ => continue,
                    };
                    if sub_tree.is_empty() {
                        out.insert(k, v);
                    } else {
                        out.insert(k, apply_tree(v, sub_tree));
                    }
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => {
            // Arrays: apply the same selection to each element.
            Value::Array(items.into_iter().map(|v| apply_tree(v, tree)).collect())
        }
        // Scalars (or null) pass through unchanged; the filter simply doesn't apply.
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn f(s: &str) -> Vec<String> {
        s.split(',').map(|x| x.to_string()).collect()
    }

    #[test]
    fn object_keeps_requested_key() {
        let v = json!({"a": 1, "b": 2});
        assert_eq!(filter_fields(v, &f("a")), json!({"a": 1}));
    }

    #[test]
    fn object_with_missing_key_returns_empty_object() {
        let v = json!({"a": 1, "b": 2});
        assert_eq!(filter_fields(v, &f("missing")), json!({}));
    }

    #[test]
    fn array_filters_each_element() {
        let v = json!([{"a": 1, "b": 2}, {"a": 3, "b": 4}]);
        assert_eq!(filter_fields(v, &f("a")), json!([{"a": 1}, {"a": 3}]));
    }

    #[test]
    fn dotted_path_recurses() {
        let v = json!({"x": {"y": 1, "z": 2}, "other": 3});
        assert_eq!(filter_fields(v, &f("x.y")), json!({"x": {"y": 1}}));
    }

    #[test]
    fn shared_prefix_keeps_both() {
        let v = json!({"x": {"y": 1, "z": 2, "q": 3}});
        assert_eq!(
            filter_fields(v, &f("x.y,x.z")),
            json!({"x": {"y": 1, "z": 2}})
        );
    }

    #[test]
    fn envelope_is_unwrapped_into_items() {
        // The "filter items inside results, not the envelope" pitfall.
        let v = json!({
            "foods": [{"foodName": "a", "id": "1"}],
            "locale": "nb",
        });
        let out = filter_fields(v, &f("foods.foodName,locale"));
        assert_eq!(out, json!({"foods": [{"foodName": "a"}], "locale": "nb"}));
    }

    #[test]
    fn scalar_passes_through() {
        let v = json!(42);
        assert_eq!(filter_fields(v.clone(), &f("anything")), v);
        let s = json!("hello");
        assert_eq!(filter_fields(s.clone(), &f("whatever.x")), s);
        let n = json!(null);
        assert_eq!(filter_fields(n.clone(), &f("x")), n);
    }

    #[test]
    fn dotted_path_on_non_object_passes_through_that_value() {
        // `energy.quantity` on a scalar energy value — the dotted part is a
        // no-op and the scalar is kept because its key matched.
        let v = json!({"energy": 42, "other": 1});
        assert_eq!(
            filter_fields(v, &f("energy.quantity")),
            json!({"energy": 42})
        );
    }

    #[test]
    fn preserves_key_order() {
        let v = json!({"c": 1, "a": 2, "b": 3});
        let out = filter_fields(v, &f("a,b,c"));
        let keys: Vec<&str> = out
            .as_object()
            .unwrap()
            .keys()
            .map(|s| s.as_str())
            .collect();
        // Output order must follow INPUT order, not the field spec order.
        assert_eq!(keys, vec!["c", "a", "b"]);
    }

    #[test]
    fn broad_selection_subsumes_narrow() {
        // If both `x` and `x.y` are requested, `x` wins — keep the full subtree.
        let v = json!({"x": {"y": 1, "z": 2}});
        assert_eq!(
            filter_fields(v.clone(), &f("x.y,x")),
            json!({"x": {"y": 1, "z": 2}})
        );
        // Order-independent.
        assert_eq!(
            filter_fields(v, &f("x,x.y")),
            json!({"x": {"y": 1, "z": 2}})
        );
    }

    #[test]
    fn empty_and_whitespace_paths_are_ignored() {
        let v = json!({"a": 1, "b": 2});
        let fields = vec!["a".to_string(), "".to_string(), "  ".to_string()];
        assert_eq!(filter_fields(v, &fields), json!({"a": 1}));
    }

    #[test]
    fn is_stdout_tty_returns_bool() {
        // Under `cargo test` stdout is not a terminal; this just exercises the call.
        let _ = is_stdout_tty();
    }
}
