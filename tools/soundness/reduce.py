"""Greedily reduce failing schemas from a soundness run, one per failure signature.

Usage: reduce.py <jsonschema2avro> <examples.json> [max_signatures]
"""
import copy, json, re, sys

exe, examples_path = sys.argv[1], sys.argv[2]
limit = int(sys.argv[3]) if len(sys.argv) > 3 else 40
sys.argv = [sys.argv[0], exe, "0", "0"]
import soundness as sd  # noqa: E402


def signature(kind):
    if kind is None:
        return None
    kind = kind.split("  <= ")[0]
    return re.sub(r"\$[^ ]*|'[^']*'|\"[^\"]*\"|\[[^]]*\]|\{.*|\d+|s\d+\.", "_", kind)[:70]


def reproduces(schema, sig):
    for seed in range(6):
        sd.rng.seed(seed)
        if signature(sd.check(schema, tries=12)) == sig:
            return True
    return False


def candidates(node):
    """Yield (description, mutated copy) for one-step simplifications of node."""
    paths = []

    def walk(v, path):
        if isinstance(v, dict):
            for k, x in v.items():
                paths.append((path, k, "key"))
                walk(x, path + [k])
        elif isinstance(v, list):
            for i, x in enumerate(v):
                paths.append((path, i, "item"))
                walk(x, path + [i])
    walk(node, [])
    for path, k, what in paths:
        root = copy.deepcopy(node)
        parent = root
        for p in path:
            parent = parent[p]
        if what == "key":
            if k in ("type",) and path and path[-1] not in ("properties", "$defs"):
                continue
            del parent[k]
            yield root
            if isinstance(node_at(node, path + [k]), dict) and path and path[-1] in ("properties", "$defs", "items"):
                root2 = copy.deepcopy(node)
                tgt = node_at(root2, path)
                tgt[k] = {"type": "string"}
                yield root2
        else:
            if isinstance(parent, list) and len(parent) > 1:
                del parent[k]
                yield root


def node_at(v, path):
    for p in path:
        v = v[p]
    return v


def reduce(schema, sig):
    changed = True
    while changed:
        changed = False
        for cand in candidates(schema):
            if len(json.dumps(cand)) < len(json.dumps(schema)) and reproduces(cand, sig):
                schema = cand
                changed = True
                break
    return schema


examples = json.load(open(examples_path))
done = {}
for kind, schema in examples.items():
    sig = signature(kind)
    if sig in done or len(done) >= limit:
        continue
    if not reproduces(schema, sig):
        continue
    small = reduce(schema, sig)
    sd.rng.seed(0)
    done[sig] = (small, sd.check(small, tries=12))
for sig, (small, kind) in done.items():
    print("==", sig)
    print("   schema:", json.dumps(small))
    print("   failure:", kind)
