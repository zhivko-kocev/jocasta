#!/bin/sh
# Installs the latest Jocasta release for linux-x64 or darwin-arm64 (0049).
# Folder: $JOCASTA_INSTALL_DIR, else ~/.local/share/jocasta; appended to PATH in the shell rc file.
# Source: the latest GitHub release. Its archive must match the SHA-256 in the repository at the release's tag (0484):
# the release workflow commits `checksums.sha256` on top of the release's tag and tags that commit `checksums-<tag>`,
# so a replaced release asset is refused; the hash never comes from the release itself. With $JOCASTA_INSTALL_FROM set
# to a folder holding jocasta-<target>.tar.gz and its .sha256 (a local build's dist/), that archive is installed
# instead (0487). Tests only: JOCASTA_INSTALL_NO_PATH=1 leaves the rc file alone, and install.json then names none.
set -eu

repo="zhivko-kocev/jocasta"
dir="${JOCASTA_INSTALL_DIR:-$HOME/.local/share/jocasta}"
source_dir="${JOCASTA_INSTALL_FROM:-}"

case "$(uname -s)-$(uname -m)" in
	Linux-x86_64) target="linux-x64" ;;
	Darwin-arm64) target="darwin-arm64" ;;
	*) echo "jocasta install: no release for $(uname -s)-$(uname -m)" >&2; exit 1 ;;
esac
asset="jocasta-$target.tar.gz"

# The SHA-256 a sha256sum-style file (`<hash>  <name>` per line) gives $asset; empty when it names none.
expected_hash() {
	awk -v asset="$asset" '{ name = $2; sub(/^\*/, "", name) } name == asset && $1 ~ /^[0-9A-Fa-f]+$/ && length($1) == 64 { print tolower($1); exit }' "$1"
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
if [ -n "$source_dir" ]; then
	# A local build (0487): its archive and the .sha256 the build wrote beside it.
	for file in "$asset" "$asset.sha256"; do
		if [ ! -f "$source_dir/$file" ]; then
			echo "jocasta install: $source_dir has no $file; JOCASTA_INSTALL_FROM names a folder holding $asset and $asset.sha256." >&2
			exit 1
		fi
		cp "$source_dir/$file" "$tmp/$file"
	done
	where="$source_dir/$asset.sha256"
	expected="$(expected_hash "$tmp/$asset.sha256")"
	echo "jocasta install: installing $source_dir/$asset."
else
	# The latest release's archive, checked against the hash committed in the repository at its tag (0484).
	tag="$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)"
	case "$tag" in
		"" | v | [!v]* | *[!0-9A-Za-z._-]*) echo "jocasta install: the latest release's tag '$tag' isn't a version tag." >&2; exit 1 ;;
	esac
	curl -fsSL "https://github.com/$repo/releases/download/$tag/$asset" -o "$tmp/$asset"
	curl -fsSL "https://raw.githubusercontent.com/$repo/checksums-$tag/checksums.sha256" -o "$tmp/checksums.sha256"
	where="the repository's checksums-$tag tag"
	expected="$(expected_hash "$tmp/checksums.sha256")"
fi
if [ -z "$expected" ]; then
	echo "jocasta install: $where has no SHA-256 for $asset; nothing was installed." >&2
	exit 1
fi
if command -v sha256sum >/dev/null 2>&1; then
	actual="$(sha256sum "$tmp/$asset" | cut -d ' ' -f 1)"
else
	actual="$(shasum -a 256 "$tmp/$asset" | cut -d ' ' -f 1)"
fi
if [ "$actual" != "$expected" ]; then
	echo "jocasta install: $asset doesn't match the SHA-256 in $where (expected $expected, got $actual); nothing was installed." >&2
	exit 1
fi

mkdir -p "$dir"
tar -xzf "$tmp/$asset" -C "$dir"
dir="$(cd "$dir" && pwd -P)"
case "$(basename "${SHELL:-}")" in
	zsh) rc="$HOME/.zshrc" ;;
	bash) rc="$HOME/.bashrc" ;;
	*) rc="$HOME/.profile" ;;
esac
json() { printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'; }
# The installer's marker (F-14): only the install it names is the official one. It also names the rc file whose PATH
# line `jocasta uninstall` removes (0468).
if [ "${JOCASTA_INSTALL_NO_PATH:-}" = "1" ]; then
	printf '{"dir":"%s"}\n' "$(json "$dir")" >"$dir/install.json"
else
	printf '{"dir":"%s","rc":"%s"}\n' "$(json "$dir")" "$(json "$rc")" >"$dir/install.json"
	line="export PATH=\"$dir:\$PATH\" # jocasta"
	if ! grep -qsF "$line" "$rc"; then
		printf '\n%s\n' "$line" >>"$rc"
		echo "jocasta install: added $dir to PATH in $rc; open a new shell to use it."
	fi
fi
"$dir/jocasta" --version
