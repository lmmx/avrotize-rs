# Rust release workflow using release-plz
#   just ship          -> the version release-plz proposes
#   just ship 0.2.0    -> this version instead (release-plz set-version)
ship version="":
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
    
    # 🔍 Refuse to run on a dirty working tree: the release commit must hold only the release
    if [[ -n "$(git status --porcelain)" ]]; then
        # ❌ Refusing to run: the working tree is not clean
        git status --short
        exit 1
    fi

    # 🦀 Update Cargo.toml versions and changelogs
    release-plz update
    if [[ -n "{{version}}" ]]; then
        # 🦀 Setting the version to {{version}}
        release-plz set-version "{{version}}"
    fi
    # Run the lint fixers before staging, so the release commit holds what they change
    just precommit || true
    git add --all
    git commit -m "chore(release): 🦀 Upgrades"
    # 🔍 Stop before pushing if anything (a hook or fixer) changed files after the commit
    if [[ -n "$(git status --porcelain)" ]]; then
        # ❌ The working tree changed after the release commit; nothing was pushed
        git status --short
        exit 1
    fi
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
