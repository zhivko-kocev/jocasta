#!/bin/sh
# Installs or upgrades the latest Jocasta release for linux-x64 or darwin-arm64 (0049).
# Folder: $JOCASTA_INSTALL_DIR, else ~/.local/share/jocasta; appended to PATH in the shell rc file. The folder is
# Jocasta's alone (0579): the file system's root, or a folder holding files when no install.json there names that
# same folder, is refused.
# Source: the latest GitHub release. Its archive must match the SHA-256 in the repository at the release's tag (0484):
# the release workflow commits `checksums.sha256` on top of the release's tag and tags that commit `checksums-<tag>`,
# so a replaced release asset is refused; the hash never comes from the release itself. With $JOCASTA_INSTALL_FROM set
# to a folder holding jocasta-<target>.tar.gz and its .sha256 (a local build's dist/), that archive is installed
# instead (0487). Tests only: JOCASTA_INSTALL_NO_PATH=1 leaves the rc file alone, and install.json then names none.
# Upgrade (0579): the release is unpacked complete beside the install (`<dir>.new-<stamp>`), then the old install is
# moved aside, the new one moved in and the old one removed, so no stale file stays and an interrupted unpack leaves
# the install as it was. A running program keeps its old files until it exits.
# install.json names the folder, the release's top-level entries (what `jocasta uninstall` removes) and the rc file.
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

json() { printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'; }

# Jocasta's alone (0579): the file system's root is refused, and so is a folder with anything in it unless it holds
# the install.json naming it. An existing folder is read as a full path without links.
if [ -d "$dir" ]; then dir="$(cd "$dir" && pwd -P)"; fi
if [ "$dir" = "/" ]; then
	echo "jocasta install: / is the file system's root; choose a folder for JOCASTA_INSTALL_DIR." >&2
	exit 1
fi
if [ -d "$dir" ] && [ -n "$(ls -A "$dir")" ] && ! grep -qsF "\"dir\":\"$(json "$dir")\"" "$dir/install.json"; then
	echo "jocasta install: $dir holds files that aren't Jocasta's; choose an empty folder for JOCASTA_INSTALL_DIR." >&2
	exit 1
fi

# The SHA-256 a sha256sum-style file (`<hash>  <name>` per line) gives $asset; empty when it names none.
expected_hash() {
	awk -v asset="$asset" '{ name = $2; sub(/^\*/, "", name) } name == asset && $1 ~ /^[0-9A-Fa-f]+$/ && length($1) == 64 { print tolower($1); exit }' "$1"
}

stage=""
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"; [ -z "$stage" ] || rm -rf "$stage"' EXIT
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

# The whole release, staged beside the install and checked before the install is touched. A new folder's parent is
# made now, and the folder named as a full path without links.
if [ ! -d "$dir" ]; then
	mkdir -p "$(dirname "$dir")"
	dir="$(cd "$(dirname "$dir")" && pwd -P | sed 's|/*$||')/$(basename "$dir")"
fi
stamp="$(date +%Y%m%d%H%M%S)"
stage="$dir.new-$stamp"
aside="$dir.old-$stamp"
mkdir "$stage"
tar -xzf "$tmp/$asset" -C "$stage"
if [ ! -f "$stage/jocasta" ]; then
	echo "jocasta install: $asset has no jocasta; nothing was installed." >&2
	exit 1
fi
entries=""
for path in "$stage"/* "$stage"/.[!.]* "$stage"/..?*; do
	[ -e "$path" ] || [ -L "$path" ] || continue
	entries="$entries${entries:+,}\"$(json "$(basename "$path")")\""
done
case "$(basename "${SHELL:-}")" in
	zsh) rc="$HOME/.zshrc" ;;
	bash) rc="$HOME/.bashrc" ;;
	*) rc="$HOME/.profile" ;;
esac
# The installer's marker (F-14): only the install it names is the official one. It also names what the release put in
# the folder and the rc file whose PATH line `jocasta uninstall` removes (0468, 0579).
if [ "${JOCASTA_INSTALL_NO_PATH:-}" = "1" ]; then
	printf '{"dir":"%s","entries":[%s]}\n' "$(json "$dir")" "$entries" >"$stage/install.json"
else
	printf '{"dir":"%s","entries":[%s],"rc":"%s"}\n' "$(json "$dir")" "$entries" "$(json "$rc")" >"$stage/install.json"
fi

# The swap: the old install aside, the new one in, the old one gone; a failed move puts the old one back.
if [ -e "$dir" ]; then
	mv "$dir" "$aside"
	if ! mv "$stage" "$dir"; then
		mv "$aside" "$dir"
		echo "jocasta install: couldn't move the new release into $dir; the installed Jocasta is as it was." >&2
		exit 1
	fi
	rm -rf "$aside"
else
	mv "$stage" "$dir"
fi

if [ "${JOCASTA_INSTALL_NO_PATH:-}" != "1" ]; then
	line="export PATH=\"$dir:\$PATH\" # jocasta"
	if ! grep -qsF "$line" "$rc"; then
		printf '\n%s\n' "$line" >>"$rc"
		echo "jocasta install: added $dir to PATH in $rc; open a new shell to use it."
	fi
fi
"$dir/jocasta" --version
