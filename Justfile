import ".just/commit.just"
import ".just/hooks.just"

test:
    cargo test -F cli

[working-directory: 'tools/avrotize-gen']
setup-gen:
    #!/usr/bin/env -S echo-comment --color bold-yellow
    # Creating new virtual env at $PWD
    uv venv
    source .venv/bin/activate
    uv sync

# Refresh Avro reference outputs from JSON Schema inputs
refresh-gen:
    #!/usr/bin/env -S echo-comment --shell-flags="-eu" --color bold-yellow
    if [ ! -d tools/avrotize-gen/.venv ]; then
        just setup-gen
    fi
    source tools/avrotize-gen/.venv/bin/activate
    FIXTURES_DIR=$(pwd)/tests/fixtures
    mkdir -p $FIXTURES_DIR/avro
    # Iterating through $FIXTURES_DIR/jsonschema/*.json
    for jsonschema in $FIXTURES_DIR/jsonschema/*.json; do
        name=$(basename $jsonschema .json)
        avro=$FIXTURES_DIR/avro/$name.avsc
        # Generating $jsonschema -> $avro
        avrotize j2a $jsonschema --out $avro
    done

# Diff a fixture file against its snapshot
difft fixture:
    #!/usr/bin/env -S echo-comment --shell-flags="-eu" --color bold-yellow
    fixture="{{fixture}}"
    stem=$(basename $fixture .avsc)
    snapshot=tests/snapshots/cli__${stem}.snap
    # Diff of $fixture -> $snapshot
    difft {{fixture}} <(sed '1,/^---/d' $snapshot)

# Diff all fixture files against their snapshots
difft-all:
    #!/usr/bin/env -S echo-comment --shell-flags="-eu" --color bold-yellow
    for avsc in tests/fixtures/avro/*.avsc; do
        just difft $avsc
    done
