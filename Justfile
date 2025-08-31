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
