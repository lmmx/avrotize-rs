"""Soundness fuzzer for jsonschema2avro.

For random JSON Schemas covering the #8 checklist, check that:
  1. the converter exits cleanly;
  2. fastavro parses the Avro output (names defined once, references resolvable,
     unions well formed);
  3. JSON instances that `jsonschema` confirms are VALID under the JSON Schema
     can be written with the Avro schema without loss: every object key lands in a
     record field or map, every value in a matching type, union branch or enum symbol.

Usage: soundness.py <jsonschema2avro> <n> <seed> [outdir]
Prints a tally of failure kinds and writes one example schema per failure message to
outdir. AVROCHECK names a strict parser (see run.sh); SAFE_KEYS=1 leaves out property
names that aren't valid Avro names.
"""
import collections, copy, json, os, random, subprocess, sys, tempfile

import fastavro
import jsonschema

EXE, N, SEED = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
OUT = sys.argv[4] if len(sys.argv) > 4 else None
STRICT = os.environ.get("AVROCHECK")  # apache-avro (Rust) parser: validates names strictly
import re
def avro_name(s):  # avrotize's symbol normalization, to translate enum data
    v = re.sub(r"[^a-zA-Z0-9_]", "_", s)
    if v and v[0].isdigit(): v = "_" + v
    if not v or not (v[0].isascii() and (v[0].isalpha() or v[0] == "_")): v = "_" + v
    return v
rng = random.Random(SEED)
KEYS = ["r", "d", "a", "kind", "value", "x-y", "1st"] if not os.environ.get("SAFE_KEYS") else ["r", "d", "a", "kind", "value", "b", "c"]
FORMATS = ["date-time", "date", "time", "uuid", "email", "uri", None, None, None]


# ---------------------------------------------------------------- schema generator
def gen(depth, defs):
    roll = rng.random()
    if depth <= 0 or roll < 0.30:
        return prim()
    if roll < 0.55:
        return obj(depth, defs)
    if roll < 0.68:
        s = {"type": "array"}
        if rng.random() < 0.9:
            s["items"] = gen(depth - 1, defs)
        if rng.random() < 0.15:
            s["maxItems"] = rng.randint(1, 4)
        return s
    if roll < 0.80:
        return {rng.choice(["anyOf", "oneOf"]): [gen(depth - 1, defs) for _ in range(rng.randint(2, 3))]}
    if roll < 0.86:
        base = obj(depth, defs)
        return {"allOf": [base, obj(depth, defs)]}
    if roll < 0.92 and defs is not None:
        name = "D%d" % len(defs)
        defs[name] = obj(depth - 1, defs)
        return {"$ref": "#/$defs/" + name}
    return obj(depth, defs)


def prim():
    r = rng.random()
    if r < 0.08:
        return {"enum": rng.sample(["a", "b", "c-d", "1x"], rng.randint(1, 3))}
    if r < 0.12:
        return {"const": rng.choice(["u", 7, True])}
    if r < 0.20:
        return {"type": [rng.choice(["string", "integer", "number", "boolean"]), "null"]}
    if r < 0.24:
        return {"type": rng.sample(["string", "integer", "boolean", "null"], 3)}
    t = rng.choice(["string", "integer", "number", "boolean", "null"])
    s = {"type": t}
    if t == "string":
        f = rng.choice(FORMATS)
        if f:
            s["format"] = f
        if rng.random() < 0.15:
            s["maxLength"] = 5
    if t in ("integer", "number") and rng.random() < 0.2:
        s["minimum"] = 0
    if t == "integer" and rng.random() < 0.1:
        s["format"] = "int64"
    return s


def obj(depth, defs):
    s = {"type": "object"}
    if rng.random() < 0.8:
        props = {k: gen(depth - 1, defs) for k in rng.sample(KEYS, rng.randint(0, 3))}
        s["properties"] = props
        req = [k for k in props if rng.random() < 0.5]
        if req:
            s["required"] = req
    r = rng.random()
    if r < 0.12:
        s["additionalProperties"] = False
    elif r < 0.20:
        s["additionalProperties"] = True
    elif r < 0.28:
        s["additionalProperties"] = gen(depth - 1, defs)
    if rng.random() < 0.06:
        s["patternProperties"] = {"^p": prim()}
    if rng.random() < 0.08:
        s[rng.choice(["minProperties", "maxProperties"])] = rng.randint(1, 3)
    if rng.random() < 0.05:
        s["description"] = "an object"
    return s


# ---------------------------------------------------------------- instance generator
def resolve(s, root):
    while isinstance(s, dict) and "$ref" in s:
        node = root
        for part in s["$ref"].lstrip("#/").split("/"):
            if not isinstance(node, dict) or part not in node:
                return {}
            node = node[part]
        s = node
    return s


def instance(s, root, depth=0):
    """A random instance attempt for schema s (may be invalid; checked afterwards)."""
    s = resolve(s, root)
    if s is True or s == {}:
        return rng.choice([1, "s", None, {"k": 1}, [1]])
    if not isinstance(s, dict):
        return None
    if "const" in s:
        return s["const"]
    if "enum" in s:
        return rng.choice(s["enum"])
    for k in ("anyOf", "oneOf"):
        if k in s:
            return instance(rng.choice(s[k]), root, depth + 1)
    if "allOf" in s:
        merged = {}
        for part in s["allOf"]:
            part = resolve(part, root)
            for kk, vv in part.items():
                if kk == "properties":
                    merged.setdefault("properties", {}).update(vv)
                elif kk == "required":
                    merged.setdefault("required", []).extend(vv)
                else:
                    merged[kk] = vv
        return instance(merged, root, depth + 1)
    t = s.get("type")
    if isinstance(t, list):
        t = rng.choice(t)
    if t == "null":
        return None
    if t == "boolean":
        return rng.choice([True, False])
    if t == "integer":
        return rng.choice([0, 3, 9, 2**40])
    if t == "number":
        return rng.choice([0.5, 3, 1e10])
    if t == "string":
        return {"date-time": "2020-01-01T00:00:00Z", "date": "2020-01-01", "time": "12:00:00",
                "uuid": "123e4567-e89b-12d3-a456-426614174000", "email": "a@b.c",
                "uri": "http://x"}.get(s.get("format"), rng.choice(["s", "abc"]))
    if t == "array":
        if depth > 6:
            return []
        return [instance(s.get("items", {}), root, depth + 1) for _ in range(rng.randint(0, 2))]
    if t == "object" or "properties" in s:
        out = {}
        if depth > 6:
            return out
        props = s.get("properties", {})
        for k, v in props.items():
            if k in s.get("required", []) or rng.random() < 0.6:
                out[k] = instance(v, root, depth + 1)
        extra = s.get("additionalProperties", True)
        if extra is not False and rng.random() < 0.5:
            out["extra"] = instance(extra if isinstance(extra, dict) else {}, root, depth + 1)
        pattern = s.get("patternProperties") or {}
        if pattern and rng.random() < 0.5:
            out["pk"] = instance(next(iter(pattern.values())), root, depth + 1)
        return out
    return rng.choice([1, "s", None])


# ---------------------------------------------------------------- Avro fit check
PRIMS = {"null", "boolean", "int", "long", "float", "double", "bytes", "string"}


def named_types(avro, names, ns=None):
    if isinstance(avro, dict):
        t = avro.get("type")
        if t in ("record", "enum", "fixed") and "name" in avro:
            space = avro.get("namespace", ns)
            full = avro["name"] if "." in avro["name"] or not space else space + "." + avro["name"]
            names.setdefault(full, avro)
            ns = space
        for k, v in avro.items():
            if k not in ("default", "const"):
                named_types(v, names, ns)
    elif isinstance(avro, list):
        for v in avro:
            named_types(v, names, ns)
    return names


def why(avro, datum, names, ns=None, seen=0, path="$"):
    """None if datum can be written with avro, else the first reason it can't."""
    if seen > 60:
        return None  # recursive type; assume fine past this depth
    if isinstance(avro, str):
        ok = {"null": lambda d: d is None, "boolean": lambda d: isinstance(d, bool),
              "int": lambda d: isinstance(d, int) and not isinstance(d, bool) and -2**31 <= d < 2**31,
              "long": lambda d: isinstance(d, int) and not isinstance(d, bool) and -2**63 <= d < 2**63,
              "float": lambda d: isinstance(d, (int, float)) and not isinstance(d, bool),
              "double": lambda d: isinstance(d, (int, float)) and not isinstance(d, bool),
              "string": lambda d: isinstance(d, str), "bytes": lambda d: isinstance(d, str)}
        if avro in ok:
            return None if ok[avro](datum) else f"type: {path} {json.dumps(datum)[:30]} not {avro}"
        full = avro if "." in avro or not ns else ns + "." + avro
        target = names.get(full) or names.get(avro)
        if target is None:
            return f"unknown: {path} {avro}"
        return why(target, datum, names, ns, seen + 1, path)
    if isinstance(avro, list):
        reasons = [why(b, datum, names, ns, seen + 1, path) for b in avro]
        if any(r is None for r in reasons):
            return None
        extra = [r for r in reasons if r.startswith("extra")]
        return extra[0] if extra else f"union: {path} {json.dumps(datum)[:40]} fits no branch of {json.dumps(avro)[:80]}"
    if isinstance(avro, dict):
        t = avro.get("type"); space = avro.get("namespace", ns)
        if t == "record":
            if not isinstance(datum, dict):
                return f"type: {path} {json.dumps(datum)[:30]} not record {avro.get('name')}"
            fields = {f["name"]: f for f in avro.get("fields", [])}
            missing = [k for k in datum if k not in fields]
            for n, f in fields.items():
                r = why(f["type"], datum.get(n), names, space, seen + 1, path + "." + n)
                if r:
                    return r
            if missing:
                return f"extra: {path} key {missing[0]!r} has no field in record {avro.get('name')} (fields {list(fields)[:4]})"
            return None
        if t == "map":
            if not isinstance(datum, dict):
                return f"type: {path} not a map"
            for k, v in datum.items():
                r = why(avro["values"], v, names, space, seen + 1, path + "." + k)
                if r:
                    return r
            return None
        if t == "array":
            if not isinstance(datum, list):
                return f"type: {path} not an array"
            for i, v in enumerate(datum):
                r = why(avro["items"], v, names, space, seen + 1, f"{path}[{i}]")
                if r:
                    return r
            return None
        if t == "enum":
            return None if isinstance(datum, str) and avro_name(datum) in avro.get("symbols", []) else f"enum: {path} {json.dumps(datum)} not in {avro.get('symbols')}"
        if t == "fixed":
            return None
        if t in PRIMS or isinstance(t, (list, dict)):
            if avro.get("logicalType") in ("date", "time-millis", "timestamp-millis") and isinstance(datum, str):
                # a string the writer translates to the logical type's number
                return None
            return why(t, datum, names, space, seen + 1, path)
        return f"shape: {path} unknown avro {json.dumps(avro)[:60]}"
    return f"shape: {path} unknown avro {avro!r}"


# ---------------------------------------------------------------- run
def reduce_key(kind):
    return kind.split(":")[0]


def check(schema, tries=8):
    """The failure kind for schema, or None. Instances are drawn from the global rng."""
    tmp = tempfile.mkdtemp()
    path = os.path.join(tmp, "s.json")
    json.dump(schema, open(path, "w"))
    out = path + ".avsc"
    p = subprocess.run([EXE, path, out], capture_output=True, timeout=60)
    if p.returncode != 0:
        return "crash: " + (p.stderr.decode().strip().splitlines() or ["?"])[-1][:120]
    avro = json.load(open(out))
    try:
        fastavro.parse_schema(copy.deepcopy(avro))
    except Exception as e:
        return "invalid avro: " + type(e).__name__ + ": " + str(e)[:100]
    if STRICT:
        q = subprocess.run([STRICT], input=json.dumps(avro) + "\n", capture_output=True, text=True)
        if not q.stdout.startswith("OK"):
            return "strict avro: " + q.stdout.strip()[4:][:100]
    if not isinstance(avro, dict):
        return None
    names = named_types(avro, {})
    jv = jsonschema.Draft202012Validator(schema)
    for _ in range(tries):
        try:
            datum = instance(schema, schema)
            if not jv.is_valid(datum):
                continue
        except Exception:
            continue
        r = why(avro, datum, names)
        fields = avro.get("fields", [])
        if r and len(fields) == 1 and why(avro, {fields[0]["name"]: datum}, names) is None:
            r = None
        if r:
            return r + "  <= " + json.dumps(datum)[:80]
    return None


def main():
    tally = collections.Counter()
    examples = {}
    tmp = tempfile.mkdtemp()
    for i in range(N):
        defs = {} if rng.random() < 0.4 else None
        schema = obj(rng.randint(2, 4), defs)
        if defs:
            schema["$defs"] = defs
        path = os.path.join(tmp, f"s{i}.json")
        json.dump(schema, open(path, "w"))
        out = path + ".avsc"
        p = subprocess.run([EXE, path, out], capture_output=True, timeout=60)
        kind = None
        if p.returncode != 0:
            kind = "crash: " + p.stderr.decode()[-160:].strip().splitlines()[-1][:120]
        else:
            avro = json.load(open(out))
            try:
                fastavro.parse_schema(copy.deepcopy(avro))
            except Exception as e:
                kind = "invalid avro: " + type(e).__name__ + ": " + str(e)[:100]
            if kind is None and STRICT:
                q = subprocess.run([STRICT], input=json.dumps(avro) + "\n", capture_output=True, text=True)
                if not q.stdout.startswith("OK"):
                    kind = "strict avro: " + q.stdout.strip()[4:][:100]
            if kind is None:
                names = named_types(avro, {})
                jv = jsonschema.Draft202012Validator(schema)
                for _ in range(8):
                    datum = instance(schema, schema)
                    if not jv.is_valid(datum):
                        continue
                    root = avro if isinstance(avro, dict) else None
                    if root is None:
                        break
                    r = why(root, datum, names)
                    fields = root.get("fields", [])
                    if r and len(fields) == 1:
                        # a wrapped root: the instance is the wrapper's one field
                        r2 = why(root, {fields[0]["name"]: datum}, names)
                        r = None if r2 is None else r
                    if r:
                        kind = r
                        break
        if kind:
            tally[reduce_key(kind)] += 1
            examples.setdefault(kind, schema)
        else:
            tally["ok"] += 1

    for k, v in tally.most_common():
        print(f"{v:6d}  {k}")
    print("distinct failure messages:", len(examples))
    for kind, schema in list(examples.items())[:12]:
        print("--", kind)
        print("  ", json.dumps(schema)[:300])
    if OUT:
        os.makedirs(OUT, exist_ok=True)
        json.dump({k: v for k, v in examples.items()}, open(os.path.join(OUT, "examples.json"), "w"), indent=1)


if __name__ == "__main__":
    main()
