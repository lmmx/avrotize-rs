use super::merging::merge_avro_schemas;
use serde_json::Value;

/// Flatten a union type into a simplified list of unique types.
///
/// This will:
/// - Recursively expand nested lists (e.g. unions of unions),
/// - Remove duplicates,
/// - Merge multiple `array` or `map` definitions into one.
pub fn flatten_union(type_list: &[Value], avro_schemas: &[Value]) -> Vec<Value> {
    let mut flat_list: Vec<Value> = Vec::new();

    // Expand nested lists and remove duplicates
    for t in type_list {
        if t.is_array() {
            let inner = flatten_union(t.as_array().unwrap(), avro_schemas);
            for u in inner {
                if !flat_list.contains(&u) {
                    flat_list.push(u);
                }
            }
        } else if !flat_list.contains(t) {
            flat_list.push(t.clone());
        }
    }

    // Consolidate array/map definitions
    let mut array_type: Option<Value> = None;
    let mut map_type: Option<Value> = None;
    let mut flat_list_1: Vec<Value> = Vec::new();

    for t in flat_list {
        if let Some(obj) = t.as_object() {
            if obj.get("type") == Some(&Value::String("array".to_string()))
                && obj.contains_key("items")
            {
                if let Some(existing) = array_type.take() {
                    array_type = Some(merge_avro_schemas(
                        &[existing, t.clone()],
                        avro_schemas,
                        None,
                        &mut Vec::new(),
                    ));
                } else {
                    array_type = Some(t.clone());
                    flat_list_1.push(t.clone());
                }
            } else if obj.get("type") == Some(&Value::String("map".to_string()))
                && obj.contains_key("values")
            {
                if let Some(existing) = map_type.take() {
                    map_type = Some(merge_avro_schemas(
                        &[existing, t.clone()],
                        avro_schemas,
                        None,
                        &mut Vec::new(),
                    ));
                } else {
                    map_type = Some(t.clone());
                    flat_list_1.push(t.clone());
                }
            } else if !flat_list_1.contains(&t) {
                flat_list_1.push(t.clone());
            }
        } else if !flat_list_1.contains(&t) {
            flat_list_1.push(t.clone());
        }
    }

    flat_list_1
}

/// The members of a well-formed Avro union of `members`.
///
/// Avro forbids a union directly inside a union and two members of the same type, so
/// nested unions are spliced in, repeated members dropped, and several arrays (or
/// maps) merged into one whose items (or values) are the union of theirs. Members
/// that break none of these rules are returned as they are, in order.
pub(crate) fn union_members(members: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(members.len());
    let mut keys: Vec<MemberKey> = Vec::with_capacity(members.len());
    for member in members {
        match member {
            Value::Array(inner) => {
                for m in union_members(inner) {
                    push_member(&mut out, &mut keys, m);
                }
            }
            m => push_member(&mut out, &mut keys, m),
        }
    }
    out
}

/// A union member's [`kind_hash`], and its [`value_hash`] once it was needed.
struct MemberKey {
    kind: u64,
    hash: Option<u64>,
}

/// A hash of what equal values share: their kind, and an object's `type` and `name`.
fn kind_hash(value: &Value) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    match value {
        Value::String(s) => (0u8, s).hash(&mut h),
        Value::Object(obj) => (
            1u8,
            obj.get("type").and_then(Value::as_str),
            obj.get("name").and_then(Value::as_str),
        )
            .hash(&mut h),
        Value::Array(_) => 2u8.hash(&mut h),
        _ => 3u8.hash(&mut h),
    }
    h.finish()
}

/// Whether `member` (of kind `kind`) repeats one of `members`. Only members of its kind
/// can, and of those only ones with an equal [`value_hash`] are compared in full; value
/// hashes are computed when first needed, so a member of a new kind costs no hashing.
fn is_repeated(members: &[Value], keys: &mut [MemberKey], member: &Value, kind: u64) -> bool {
    let mut hash = None;
    for (m, key) in members.iter().zip(keys.iter_mut()) {
        if key.kind != kind {
            continue;
        }
        let mine = *hash.get_or_insert_with(|| value_hash(member));
        if *key.hash.get_or_insert_with(|| value_hash(m)) == mine && m == member {
            return true;
        }
    }
    false
}

/// Add `member` to a union's `members`, unless it repeats one, merging it with an
/// array or map already there. `keys` holds each member's [`MemberKey`].
fn push_member(members: &mut Vec<Value>, keys: &mut Vec<MemberKey>, member: Value) {
    let kind = kind_hash(&member);
    if is_repeated(members, keys, &member, kind) {
        return;
    }
    for key in ["items", "values"] {
        let ty = if key == "items" { "array" } else { "map" };
        if member.get("type").and_then(Value::as_str) != Some(ty) {
            continue;
        }
        if let Some(i) = members
            .iter()
            .position(|m| m.get("type").and_then(Value::as_str) == Some(ty))
        {
            let existing = &mut members[i];
            let merged = union_members(vec![existing[key].take(), member[key].clone()]);
            existing[key] = match <[Value; 1]>::try_from(merged) {
                Ok([single]) => single,
                Err(merged) => Value::Array(merged),
            };
            keys[i] = MemberKey {
                kind: kind_hash(&members[i]),
                hash: None,
            };
            return;
        }
    }
    members.push(member);
    keys.push(MemberKey { kind, hash: None });
}

/// A hash of `value` that is equal for equal values: as `Value` equality ignores the
/// order of an object's keys, so does the hash (it sums its entries' hashes).
fn value_hash(value: &Value) -> u64 {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut h = DefaultHasher::new();
    match value {
        Value::Null => 0u8.hash(&mut h),
        Value::Bool(b) => (1u8, b).hash(&mut h),
        Value::Number(n) => {
            2u8.hash(&mut h);
            if let Some(i) = n.as_i64() {
                (0u8, i).hash(&mut h);
            } else if let Some(u) = n.as_u64() {
                (1u8, u).hash(&mut h);
            } else {
                // -0.0 == 0.0, so they hash alike
                let f = n.as_f64().unwrap_or(0.0);
                (2u8, if f == 0.0 { 0 } else { f.to_bits() }).hash(&mut h);
            }
        }
        Value::String(s) => (3u8, s).hash(&mut h),
        Value::Array(arr) => {
            (4u8, arr.len()).hash(&mut h);
            arr.iter().for_each(|v| value_hash(v).hash(&mut h));
        }
        Value::Object(obj) => {
            let entries = obj.iter().fold(0u64, |sum, (k, v)| {
                let mut e = DefaultHasher::new();
                (k, value_hash(v)).hash(&mut e);
                sum.wrapping_add(e.finish())
            });
            (5u8, obj.len(), entries).hash(&mut h);
        }
    }
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn equal_values_hash_alike() {
        // Value equality ignores key order, and -0.0 == 0.0
        let a = json!({"type": "record", "name": "r", "fields": [{"name": "x", "type": "int"}]});
        let b = json!({"fields": [{"type": "int", "name": "x"}], "name": "r", "type": "record"});
        assert_eq!(a, b);
        assert_eq!(value_hash(&a), value_hash(&b));
        assert_eq!(json!(-0.0), json!(0.0));
        assert_eq!(value_hash(&json!(-0.0)), value_hash(&json!(0.0)));
    }

    #[test]
    fn a_repeated_member_is_found_whatever_its_key_order() {
        let a = json!({"type": "record", "name": "r", "fields": []});
        let b = json!({"name": "r", "fields": [], "type": "record"});
        let c = json!({"type": "record", "name": "r", "fields": [{"name": "x", "type": "int"}]});
        let members = union_members(vec![a.clone(), json!("null"), b, c.clone(), json!("null")]);
        assert_eq!(members, [a, json!("null"), c]);
    }
}
