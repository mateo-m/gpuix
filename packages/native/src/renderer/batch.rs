//! Turning one batch of React mutations into tree operations.
//!
//! Every op is parsed and checked before any of them touches the tree, so a
//! malformed batch changes nothing rather than applying half of itself.

use napi::bindgen_prelude::*;

use super::to_element_id;
use crate::retained_tree::RetainedTree;
use crate::style::StyleDesc;

/// Parsed batch operation — typed enum for atomic validation.
/// All ops are parsed and validated BEFORE any tree mutation occurs.
/// This prevents partial application on malformed batches.
pub(super) enum BatchOp {
    CreateElement {
        id: u64,
        element_type: String,
    },
    DestroyElement {
        id: u64,
    },
    AppendChild {
        parent_id: u64,
        child_id: u64,
    },
    RemoveChild {
        parent_id: u64,
        child_id: u64,
    },
    InsertBefore {
        parent_id: u64,
        child_id: u64,
        before_id: u64,
    },
    SetStyle {
        id: u64,
        /// Boxed, or every op in the batch would be as wide as a `StyleDesc`.
        style: Box<StyleDesc>,
    },
    SetText {
        id: u64,
        content: String,
    },
    SetEventListener {
        id: u64,
        event_type: String,
        has_handler: bool,
    },
    SetRoot {
        id: u64,
    },
    SetCustomProp {
        id: u64,
        key: String,
        value: serde_json::Value,
    },
}

/// Parse all batch ops from JSON into typed enums.
/// Returns Err on the first invalid op — no tree mutation has occurred yet.
pub(super) fn parse_batch_ops(ops: &[serde_json::Value]) -> Result<Vec<BatchOp>> {
    let mut parsed = Vec::with_capacity(ops.len());

    for (i, op) in ops.iter().enumerate() {
        let arr = op
            .as_array()
            .ok_or_else(|| Error::from_reason(format!("Batch op {} is not an array", i)))?;
        let op_name = arr
            .first()
            .and_then(|v| v.as_str())
            .ok_or_else(|| Error::from_reason(format!("Batch op {} missing op name string", i)))?;

        let batch_op = match op_name {
            "createElement" => BatchOp::CreateElement {
                id: batch_id(arr, 1, i)?,
                element_type: batch_str(arr, 2, i)?,
            },
            "destroyElement" => BatchOp::DestroyElement {
                id: batch_id(arr, 1, i)?,
            },
            "appendChild" => BatchOp::AppendChild {
                parent_id: batch_id(arr, 1, i)?,
                child_id: batch_id(arr, 2, i)?,
            },
            "removeChild" => BatchOp::RemoveChild {
                parent_id: batch_id(arr, 1, i)?,
                child_id: batch_id(arr, 2, i)?,
            },
            "insertBefore" => BatchOp::InsertBefore {
                parent_id: batch_id(arr, 1, i)?,
                child_id: batch_id(arr, 2, i)?,
                before_id: batch_id(arr, 3, i)?,
            },
            "setStyle" => {
                let value = batch_payload(arr, 2, i)?;
                let style = StyleDesc::deserialize_boxed(value).map_err(|e| {
                    Error::from_reason(format!("Batch op {} setStyle parse error: {}", i, e))
                })?;
                BatchOp::SetStyle {
                    id: batch_id(arr, 1, i)?,
                    style,
                }
            }
            "setText" => BatchOp::SetText {
                id: batch_id(arr, 1, i)?,
                content: batch_str(arr, 2, i)?,
            },
            "setEventListener" => {
                let has_handler = arr
                    .get(3)
                    .and_then(|v| v.as_bool().or_else(|| v.as_u64().map(|n| n != 0)))
                    .ok_or_else(|| {
                        Error::from_reason(format!(
                            "Batch op {} setEventListener missing/invalid hasHandler at index 3",
                            i
                        ))
                    })?;
                BatchOp::SetEventListener {
                    id: batch_id(arr, 1, i)?,
                    event_type: batch_str(arr, 2, i)?,
                    has_handler,
                }
            }
            "setRoot" => BatchOp::SetRoot {
                id: batch_id(arr, 1, i)?,
            },
            "setCustomProp" => BatchOp::SetCustomProp {
                id: batch_id(arr, 1, i)?,
                key: batch_str(arr, 2, i)?,
                value: batch_payload(arr, 3, i)?,
            },
            "setCustomPropValue" => BatchOp::SetCustomProp {
                id: batch_id(arr, 1, i)?,
                key: batch_str(arr, 2, i)?,
                value: arr.get(3).cloned().ok_or_else(|| {
                    Error::from_reason(format!("Batch op {} missing custom prop value", i))
                })?,
            },
            _ => {
                return Err(Error::from_reason(format!(
                    "Batch op {} unknown operation: {:?}",
                    i, op_name
                )));
            }
        };
        parsed.push(batch_op);
    }

    Ok(parsed)
}

/// Apply a batch of mutation tuples to a RetainedTree.
/// Shared between GpuixRenderer::apply_batch and TestGpuixRenderer::apply_batch.
/// Returns accumulated destroyed IDs (as f64) from all destroyElement ops.
///
/// ATOMIC: all ops are parsed and validated first. If any op is malformed,
/// the tree is left unchanged and an error is returned. This prevents
/// partial application that could desync JS and Rust state.
///
/// Batch format: JSON array of tuples [opcode, ...args].
/// See GpuixRenderer::apply_batch for opcode documentation.
pub(crate) fn apply_batch_to_tree(
    tree: &mut RetainedTree,
    ops: &[serde_json::Value],
) -> Result<Vec<f64>> {
    // Phase 1: parse and validate all ops (no mutation).
    let parsed = parse_batch_ops(ops)?;

    // Phase 2: apply all validated ops to the tree.
    let mut destroyed_ids: Vec<f64> = Vec::new();
    for batch_op in parsed {
        match batch_op {
            BatchOp::CreateElement { id, element_type } => {
                tree.create_element(id, element_type);
            }
            BatchOp::DestroyElement { id } => {
                let destroyed = tree.destroy_element(id);
                destroyed_ids.extend(destroyed.iter().map(|&id| id as f64));
            }
            BatchOp::AppendChild {
                parent_id,
                child_id,
            } => {
                tree.append_child(parent_id, child_id);
            }
            BatchOp::RemoveChild {
                parent_id,
                child_id,
            } => {
                tree.remove_child(parent_id, child_id);
            }
            BatchOp::InsertBefore {
                parent_id,
                child_id,
                before_id,
            } => {
                tree.insert_before(parent_id, child_id, before_id);
            }
            BatchOp::SetStyle { id, style } => {
                tree.set_style(id, style);
            }
            BatchOp::SetText { id, content } => {
                tree.set_text(id, content);
            }
            BatchOp::SetEventListener {
                id,
                event_type,
                has_handler,
            } => {
                tree.set_event_listener(id, event_type, has_handler);
            }
            BatchOp::SetRoot { id } => {
                tree.root_id = Some(id);
            }
            BatchOp::SetCustomProp { id, key, value } => {
                tree.set_custom_prop(id, key, value);
            }
        }
    }

    Ok(destroyed_ids)
}

/// Extract a u64 element ID from a batch tuple at the given index.
fn batch_id(arr: &[serde_json::Value], idx: usize, op_idx: usize) -> Result<u64> {
    let v = arr.get(idx).and_then(|v| v.as_f64()).ok_or_else(|| {
        Error::from_reason(format!("Batch op {} missing id at index {}", op_idx, idx))
    })?;
    to_element_id(v)
}

/// A style or custom-prop payload. Objects land as JSON. Legacy batches
/// still send a JSON string and get decoded here.
fn batch_payload(
    arr: &[serde_json::Value],
    idx: usize,
    op_idx: usize,
) -> Result<serde_json::Value> {
    let value = arr.get(idx).ok_or_else(|| {
        Error::from_reason(format!(
            "Batch op {} missing value at index {}",
            op_idx, idx
        ))
    })?;
    if let Some(encoded) = value.as_str() {
        match serde_json::from_str(encoded) {
            Ok(parsed) => Ok(parsed),
            Err(_) => Ok(serde_json::Value::String(encoded.to_string())),
        }
    } else {
        Ok(value.clone())
    }
}

/// Extract a String from a batch tuple at the given index.
fn batch_str(arr: &[serde_json::Value], idx: usize, op_idx: usize) -> Result<String> {
    arr.get(idx)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            Error::from_reason(format!(
                "Batch op {} missing string at index {}",
                op_idx, idx
            ))
        })
}
