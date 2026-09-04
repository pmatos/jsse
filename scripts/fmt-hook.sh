#!/usr/bin/env bash
# PostToolUse hook: format edited Rust files and Clippy their Cargo target.

input=$(cat)
file_path=$(jq -r '.tool_input.file_path // empty' <<<"$input" 2>/dev/null) ||
    exit 0

if [[ "$file_path" != *.rs ]]; then
    exit 0
fi

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)
if [[ "$file_path" != /* ]]; then
    file_path="$repo_root/$file_path"
fi

if [[ ! -f "$file_path" ]]; then
    exit 0
fi

file_dir=$(cd -- "$(dirname -- "$file_path")" 2>/dev/null && pwd -P) ||
    exit 0
file_path="$file_dir/$(basename -- "$file_path")"

if ! fmt_output=$(cargo fmt --manifest-path "$repo_root/Cargo.toml" -- "$file_path" 2>&1); then
    printf 'rustfmt failed for %s:\n%s\n' "$file_path" "$fmt_output" >&2
    exit 2
fi

case "$file_path" in
    "$repo_root/src/lib.rs")
        target_args=(--lib)
        ;;
    "$repo_root/src/bin/"*.rs)
        target_path=${file_path#"$repo_root/src/bin/"}
        target_name=${target_path%%/*}
        target_name=${target_name%.rs}
        target_args=(--bin "$target_name")
        ;;
    "$repo_root/src/"*.rs | "$repo_root/build.rs")
        # --all-targets so Clippy also compiles the cfg(test) build; a bare
        # --bin jsse skips #[cfg(test)] source (e.g. src/interpreter/tests.rs)
        # and would report success without ever linting the edit.
        target_args=(--all-targets)
        ;;
    "$repo_root/tests/"*.rs)
        target_path=${file_path#"$repo_root/tests/"}
        if [[ "$target_path" == */* ]]; then
            target_args=(--tests)
        else
            target_args=(--test "${target_path%.rs}")
        fi
        ;;
    "$repo_root/examples/"*.rs)
        target_path=${file_path#"$repo_root/examples/"}
        target_name=${target_path%%/*}
        target_args=(--example "${target_name%.rs}")
        ;;
    "$repo_root/benches/"*.rs)
        target_path=${file_path#"$repo_root/benches/"}
        target_name=${target_path%%/*}
        target_args=(--bench "${target_name%.rs}")
        ;;
    *)
        exit 0
        ;;
esac

if ! clippy_output=$(
    cargo clippy --manifest-path "$repo_root/Cargo.toml" --quiet \
        "${target_args[@]}" -- -D warnings 2>&1
); then
    relative_path=${file_path#"$repo_root/"}
    printf 'Clippy failed for %s:\n%s\n' "$relative_path" "$clippy_output" >&2
    exit 2
fi

exit 0
