# Installs or upgrades Jocasta for windows-x64 (0049): jocasta.exe, its config\ and, on Windows, herdr's ConPTY runtime.
# Source: the latest GitHub release. Its zip must match the SHA-256 in the repository at the release's tag (0484): the
# release workflow commits `checksums.sha256` on top of the release's tag and tags that commit `checksums-<tag>`, so a
# replaced release asset is refused; the hash never comes from the release itself. With $env:JOCASTA_INSTALL_FROM set
# to a folder holding jocasta-windows-x64.zip and its .sha256 (a local build's dist\), that zip is installed instead
# (0487). Tests only: $env:JOCASTA_INSTALL_NO_PATH=1 leaves the user PATH alone, and install.json then names no PATH
# entry.
# Folder: $env:JOCASTA_INSTALL_DIR, else %LOCALAPPDATA%\Programs\jocasta\; added to the user's PATH. The folder is
# Jocasta's alone (0579): a drive root, or a folder holding files when no install.json there names that same folder,
# is refused.
# Layout: <dir>\jocasta.exe, <dir>\config\, <dir>\conpty\ with <dir>\THIRD-PARTY-NOTICES\ (Microsoft's ConPTY, 0661), and
# <dir>\install.json naming <dir>, the release's top-level entries (what
# `jocasta uninstall` removes, 0579) and what this install adds beyond the folder (0468): its user PATH entry.
# Upgrade (CR-06): Jocasta's server (`jocasta.exe server`, herdr's server with the service in it, fork plan phase 4),
# omp sessions' `jocasta.exe mcp` and panes' OpenConsole.exe run from the install, so neither
# those files nor the folder can be replaced (a running program's file can't be overwritten, and its folder can't be
# renamed). A running program's file can be renamed, though: the release is staged complete beside the install, an
# older install's companion (`app\`, gone with 0552) stops, the old install is copied aside, every file still in use
# (.exe, .dll) is renamed in place to `<name>.old-<stamp>`, and robocopy mirrors the staged release over the
# install, leaving those `.old-*` files alone. Running programs keep their old files until they exit; `.old-*` files
# no longer in use are removed now and by every later install. A running Jocasta keeps its panes and the old build
# until he restarts it: Windows has no live handoff (fork plan finding 20). If the renames or the copy fail, the
# install is mirrored back from the copy aside, so there is never half an install.
# Everything runs in its own scope, so `irm … | iex` leaves his shell's preferences as they were.
& {
	$ErrorActionPreference = 'Stop'
	$ProgressPreference = 'SilentlyContinue' # Windows PowerShell 5.1's progress bar slows Invoke-WebRequest badly

	$repo = 'zhivko-kocev/jocasta'
	$asset = 'jocasta-windows-x64.zip'
	$source = $env:JOCASTA_INSTALL_FROM
	$addToPath = $env:JOCASTA_INSTALL_NO_PATH -ne '1'
	$dir = [IO.Path]::GetFullPath($(if ($env:JOCASTA_INSTALL_DIR) { $env:JOCASTA_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\jocasta' })).TrimEnd('\')
	if ($dir -match '^[A-Za-z]:$') { throw "jocasta install: $dir is a drive root; choose a folder for JOCASTA_INSTALL_DIR." }
	# Jocasta's alone (0579): anything in the folder must come with the install.json that names this same folder.
	$owned = $false
	try {
		$named = [string](Get-Content -Raw -LiteralPath (Join-Path $dir 'install.json') | ConvertFrom-Json).dir
		$owned = $named -and ([IO.Path]::GetFullPath($named).TrimEnd('\') -ieq $dir)
	} catch { $owned = $false }
	if ((Test-Path $dir -PathType Container) -and (Get-ChildItem -Force $dir | Select-Object -First 1) -and -not $owned) {
		throw "jocasta install: $dir holds files that aren't Jocasta's; choose an empty folder for JOCASTA_INSTALL_DIR."
	}
	$stamp = Get-Date -Format 'yyyyMMddHHmmss'
	$stage = "$dir.new-$stamp"
	$aside = "$dir.previous-$stamp"

	# Whether a program holds `$path` open (a running .exe, or a loaded .dll).
	function Test-InUse([string]$path) {
		try { [IO.File]::Open($path, 'Open', 'ReadWrite', 'None').Dispose(); return $false } catch { return $true }
	}

	# `.old-*` files an upgrade renamed aside, once no program holds them.
	function Remove-UnusedOld {
		if (-not (Test-Path $dir)) { return }
		Get-ChildItem -Path $dir -Recurse -File -Filter '*.old-*' -ErrorAction SilentlyContinue |
			Where-Object { -not (Test-InUse $_.FullName) } |
			ForEach-Object { Remove-Item -Force $_.FullName -ErrorAction SilentlyContinue }
	}

	# robocopy's mirror; exit codes 8 and up are failures. `.old-*` files in the destination are neither copied nor
	# purged, and a junction is never followed (/XJ), so the purge can't reach outside the folder (0579).
	function Sync-Folder([string]$from, [string]$to) {
		robocopy $from $to /MIR /XJ /XF '*.old-*' /R:2 /W:1 /NFL /NDL /NJH /NJS /NP | Out-Null
		if ($LASTEXITCODE -ge 8) { throw "robocopy $from -> $to exited $LASTEXITCODE" }
	}

	# The SHA-256 a sha256sum-style file (`<hash>  <name>` per line) gives $asset, else a refusal naming $where.
	function Get-ExpectedHash([string]$file, [string]$where) {
		foreach ($line in Get-Content $file) {
			$fields = @($line.Trim() -split '\s+')
			if ($fields.Count -ge 2 -and $fields[1].TrimStart('*') -eq $asset -and $fields[0] -match '^[0-9A-Fa-f]{64}$') {
				return $fields[0]
			}
		}
		throw "jocasta install: $where has no SHA-256 for $asset; nothing was installed."
	}

	Remove-UnusedOld

	$tmp = Join-Path ([IO.Path]::GetTempPath()) "jocasta-install-$([guid]::NewGuid())"
	New-Item -ItemType Directory -Path $tmp | Out-Null
	try {
		if ($source) {
			# A local build (0487): its zip and the .sha256 the build wrote beside it.
			$source = [IO.Path]::GetFullPath($source).TrimEnd('\')
			foreach ($file in $asset, "$asset.sha256") {
				if (-not (Test-Path (Join-Path $source $file) -PathType Leaf)) {
					throw "jocasta install: $source has no $file; JOCASTA_INSTALL_FROM names a folder holding $asset and $asset.sha256."
				}
				Copy-Item -LiteralPath (Join-Path $source $file) -Destination (Join-Path $tmp $file)
			}
			$where = "$source\$asset.sha256"
			$expected = Get-ExpectedHash "$tmp\$asset.sha256" $where
			Write-Host "jocasta install: installing $source\$asset."
		} else {
			# The latest release's zip, checked against the hash committed in the repository at its tag (0484).
			$tag = [string](Invoke-RestMethod -UseBasicParsing "https://api.github.com/repos/$repo/releases/latest").tag_name
			if ($tag -notmatch '^v[0-9A-Za-z._-]+$') { throw "jocasta install: the latest release's tag '$tag' isn't a version tag." }
			Invoke-WebRequest -UseBasicParsing "https://github.com/$repo/releases/download/$tag/$asset" -OutFile "$tmp\$asset"
			Invoke-WebRequest -UseBasicParsing "https://raw.githubusercontent.com/$repo/checksums-$tag/checksums.sha256" -OutFile "$tmp\checksums.sha256"
			$where = "the repository's checksums-$tag tag"
			$expected = Get-ExpectedHash "$tmp\checksums.sha256" $where
		}
		$actual = (Get-FileHash "$tmp\$asset" -Algorithm SHA256).Hash
		if ($actual -ne $expected) {
			throw "jocasta install: $asset doesn't match the SHA-256 in $where (expected $($expected.ToLowerInvariant()), got $($actual.ToLowerInvariant())); nothing was installed."
		}

		# The whole release, staged and checked before the install is touched. Windows' own tar unpacks it in about a
		# tenth of Expand-Archive's time.
		New-Item -ItemType Directory -Path $stage | Out-Null
		& "$env:SystemRoot\System32\tar.exe" -xf "$tmp\$asset" -C $stage
		if ($LASTEXITCODE -ne 0) { throw "jocasta install: couldn't unpack $asset (tar exited $LASTEXITCODE); nothing was installed." }
		if (-not (Test-Path (Join-Path $stage 'jocasta.exe'))) { throw "jocasta install: $asset has no jocasta.exe" }
		$record = [ordered]@{ dir = $dir; entries = @(Get-ChildItem -Force -LiteralPath $stage | ForEach-Object { $_.Name }) }
		if ($addToPath) { $record.path = $dir }
		# What an earlier install's companion (0367, gone with 0552) may have left: uninstall removes either only while
		# it is still the companion's (its Run value naming app\bin\launcher.exe, its WebView2 data).
		$record.autostart = 'Jocasta'
		$record.appData = Join-Path $env:LOCALAPPDATA 'local.jocasta.frontier'
		[IO.File]::WriteAllText((Join-Path $stage 'install.json'), (ConvertTo-Json -Compress $record))

		$upgrading = Test-Path $dir
		$running = @()
		if ($upgrading) {
			# An older install's companion (launcher.exe and its bun.exe under app\) stops: it would keep its old
			# service's port. A running Jocasta stays: stopping its server would close his panes.
			$companion = @(Get-CimInstance Win32_Process |
				Where-Object { $_.ExecutablePath -and $_.ExecutablePath.StartsWith("$dir\app\", [StringComparison]::OrdinalIgnoreCase) })
			if ($companion.Count -gt 0) {
				Write-Host "jocasta install: stopping the companion app an earlier install left running."
				$companion | ForEach-Object { Stop-Process -Id $_.ProcessId -Force -ErrorAction SilentlyContinue }
				Start-Sleep -Milliseconds 500
			}
			$running = @(Get-CimInstance Win32_Process -Filter "Name = 'jocasta.exe'" |
				Where-Object {
					$_.ExecutablePath -and $_.ExecutablePath -ieq "$dir\jocasta.exe" -and $_.CommandLine -match '\sserver\s*$'
				})
			# The install as it is, to put back if the copy fails (running programs' files can be read).
			try { Sync-Folder $dir $aside } catch {
				Remove-Item -Recurse -Force $aside -ErrorAction SilentlyContinue
				throw "jocasta install: couldn't copy the install aside ($($_.Exception.Message)); the installed Jocasta is unchanged."
			}
		}
		try {
			if ($upgrading) {
				# Files in use move aside under a new name, so the new ones can take theirs. One in conpty\ (its
				# OpenConsole.exe runs for every open pane) moves to the install's top as conpty-<path>.old-<stamp>:
				# herdr loads the bundle only with exactly its pinned files in it (0661).
				$bundle = Join-Path $dir 'conpty'
				Get-ChildItem -Path $dir -Recurse -File -Include '*.exe', '*.dll' |
					Where-Object { $_.Name -notlike '*.old-*' -and (Test-InUse $_.FullName) } |
					ForEach-Object {
						if ($_.FullName.StartsWith("$bundle\", [StringComparison]::OrdinalIgnoreCase)) {
							$flat = $_.FullName.Substring($bundle.Length + 1).Replace('\', '-')
							Move-Item -LiteralPath $_.FullName -Destination (Join-Path $dir "conpty-$flat.old-$stamp")
						} else {
							Rename-Item -Path $_.FullName -NewName "$($_.Name).old-$stamp"
						}
					}
			}
			Sync-Folder $stage $dir
		} catch {
			$why = $_.Exception.Message
			if (Test-Path $aside) {
				try { Sync-Folder $aside $dir } catch { throw "jocasta install: the copy failed ($why) and putting the old install back failed too ($($_.Exception.Message)); it is complete in $aside." }
				Remove-Item -Recurse -Force $aside -ErrorAction SilentlyContinue
				throw "jocasta install: the copy failed ($why); the installed Jocasta is as it was."
			}
			throw "jocasta install: the copy failed ($why)."
		}
		if (Test-Path $aside) { Remove-Item -Recurse -Force $aside -ErrorAction SilentlyContinue }
		Remove-UnusedOld

		# The user Path as stored (REG_EXPAND_SZ, %VARS% unexpanded), with the install appended when missing; open
		# programs are told the environment changed. Tests leave it alone (JOCASTA_INSTALL_NO_PATH).
		if ($addToPath) {
			$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
			try {
				$userPath = [string]$key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
				$entries = @($userPath -split ';' | Where-Object { $_ })
				if ($entries -notcontains $dir) {
					$key.SetValue('Path', (($entries + $dir) -join ';'), [Microsoft.Win32.RegistryValueKind]::ExpandString)
					if (-not ('JocastaInstall.Native' -as [type])) {
						Add-Type -Namespace JocastaInstall -Name Native -MemberDefinition @'
[DllImport("user32.dll", CharSet = CharSet.Unicode)]
public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint msg, UIntPtr wParam, string lParam, uint flags, uint timeout, out UIntPtr result);
'@
					}
					$result = [UIntPtr]::Zero
					# HWND_BROADCAST, WM_SETTINGCHANGE, SMTO_ABORTIFHUNG, 5 s.
					[void][JocastaInstall.Native]::SendMessageTimeout([IntPtr]0xffff, 0x1A, [UIntPtr]::Zero, 'Environment', 2, 5000, [ref]$result)
					Write-Host "jocasta install: added $dir to the user PATH; open a new terminal to use it."
				}
			} finally {
				$key.Close()
			}
		}
		if ($running.Count -gt 0) {
			Write-Host "jocasta install: Jocasta is running on the old build; close it and run jocasta again to use the new one (Windows can't hand its panes over live)."
		} elseif (Get-ChildItem -Path $dir -Recurse -File -Filter '*.old-*' -ErrorAction SilentlyContinue) {
			Write-Host "jocasta install: omp sessions started before the upgrade run the old build until they restart."
		}
		& (Join-Path $dir 'jocasta.exe') --version
	} finally {
		Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
		if (Test-Path $stage) { Remove-Item -Recurse -Force $stage -ErrorAction SilentlyContinue }
	}
}
