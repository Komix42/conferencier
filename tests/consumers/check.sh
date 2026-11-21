#!/usr/bin/env bash
set -euo pipefail

root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$root"

metadata=$(cargo +1.85 metadata --no-deps --format-version 1 --locked)
version=$(jq -er '.packages[] | select(.name == "conferencier") | .version' <<<"$metadata")
target_dir=$(jq -er '.target_directory' <<<"$metadata")
temp_base=${RUNNER_TEMP:-${TMPDIR:-/tmp}}
temp_dir=$(mktemp -d "$temp_base/conferencier-consumers.XXXXXX")

cleanup() {
    case "$temp_dir" in
        "$temp_base"/conferencier-consumers.*) rm -rf -- "$temp_dir" ;;
        *) printf 'Refusing to remove unexpected path: %s\n' "$temp_dir" >&2; return 1 ;;
    esac
}
trap cleanup EXIT

package_dir="$temp_dir/package"
consumer_dir="$temp_dir/consumers"
mkdir -p "$package_dir" "$consumer_dir"

for crate in conferencier conferencier-derive; do
    archive="$target_dir/package/$crate-$version.crate"
    if [[ ! -f "$archive" ]]; then
        printf 'Missing packaged archive: %s\n' "$archive" >&2
        printf '%s\n' \
            'Run cargo publish --workspace --dry-run --locked and' \
            'cargo package -p conferencier -p conferencier-derive --no-verify --locked first.' >&2
        exit 1
    fi
done

tar -xzf "$target_dir/package/conferencier-$version.crate" -C "$package_dir"
tar -xzf "$target_dir/package/conferencier-derive-$version.crate" -C "$package_dir"

# Exact fixture requirements prevent a newer registry release from superseding the patch.
for consumer in derive-reexport manual-no-defaults direct-derive; do
    destination="$consumer_dir/$consumer"
    mkdir -p "$destination"
    sed "s/@CANDIDATE_VERSION@/$version/g" \
        "tests/consumers/$consumer/Cargo.toml.in" >"$destination/Cargo.toml"
    cp -R "tests/consumers/$consumer/src" "$destination/src"

    mkdir -p "$destination/.cargo"
    cat >"$destination/.cargo/config.toml" <<EOF
[patch.crates-io]
conferencier = { path = "$package_dir/conferencier-$version" }
EOF
    if [[ "$consumer" != manual-no-defaults ]]; then
        cat >>"$destination/.cargo/config.toml" <<EOF
conferencier-derive = { path = "$package_dir/conferencier-derive-$version" }
EOF
    fi

    printf 'Checking packaged consumer: %s\n' "$consumer"
    (cd "$destination" && cargo +1.85 check)
done
