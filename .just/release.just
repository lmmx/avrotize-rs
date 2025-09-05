# Rust release workflow using release-plz
ship:
    #!/usr/bin/env -S echo-comment --shell-flags="-euo pipefail" --color="\\033[38;5;202m"

    # 🔍 Refuse to run if not on master branch or not up to date with origin/master
    branch="$(git rev-parse --abbrev-ref HEAD)"
    if [[ "$branch" != "master" ]]; then
        # ❌ Refusing to run: not on 'master' branch (current: $branch)
        exit 1
    fi
    # 🔍 Fetch master branch
    git fetch origin master
    local_rev="$(git rev-parse HEAD)"
    remote_rev="$(git rev-parse origin/master)"
    # 🔍 Local: $local_rev\n🔍 Remote: $remote_rev
    if [[ "$local_rev" != "$remote_rev" ]]; then
        # ❌ Refusing to run: local master branch is not up to date with origin/master
        # Local HEAD:  $local_rev
        # Origin HEAD: $remote_rev
        # Please pull/rebase to update.
        exit 1
    fi

    # 🔍 Dry-run release...
    just publish --dry-run
    # ✅ Dry-run went OK, proceeding to real release
    
    # 🦀 Update Cargo.toml versions and changelogs
    release-plz update
    git add .
    # Run a pre-precommit lint pass to avoid the linter halting our release!
    just precommit || true
    git commit -m "chore(release): 🦀 Upgrades"
    # Note: if already pushed you would just need to revert the additions (delete changelogs)

    # 🦀 Run prepush only for the Rust crates we are releasing
    just prepush
    # 🚀 Push the version bump commit
    git push --no-verify

    # 📦 Create releases and tags
    just publish

publish mode="":
    #!/usr/bin/env -S bash -euo pipefail
    git_token=$(gh auth token 2>/dev/null) || git_token=$PUBLISH_GITHUB_TOKEN

    ## 🦀 Let release-plz handle workspace crate tagging
    ## It will create tags like: genson-core-v0.2.1, genson-cli-v0.1.5, etc.
    release-plz release --backend github --git-token $git_token {{mode}}
