$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot -Parent
$out = Join-Path $root 'apps/desktop/src-tauri/easytier'
New-Item -ItemType Directory -Force $out | Out-Null
$temp = Join-Path ([IO.Path]::GetTempPath()) ([guid]::NewGuid().ToString())
New-Item -ItemType Directory $temp | Out-Null
try {
  $zip = Join-Path $temp 'kernel.zip'
  Invoke-WebRequest 'https://github.com/EasyTier/EasyTier/releases/download/v2.6.4/easytier-windows-x86_64-v2.6.4.zip' -OutFile $zip
  if ((Get-FileHash $zip -Algorithm SHA256).Hash.ToLower() -ne '27af91e270e554709b048bd32327fefd2dfce5062ae1e8701af7550c6f525f84') { throw 'EasyTier archive SHA-256 mismatch' }
  Copy-Item $zip (Join-Path $out 'EasyTier-v2.6.4-binary.zip')
  Expand-Archive $zip (Join-Path $temp 'binary')
  $exe = Get-ChildItem (Join-Path $temp 'binary') -Recurse -Filter easytier-core.exe | Select-Object -First 1
  if (-not $exe) { throw 'Official archive missing easytier-core.exe' }
  Copy-Item $exe.FullName $out
  Get-ChildItem $exe.Directory.FullName -Filter '*.dll' | Copy-Item -Destination $out
  Invoke-WebRequest 'https://github.com/EasyTier/EasyTier/archive/8428a89d2dabc94c97d370ec607c6ca142473626.zip' -OutFile (Join-Path $out 'EasyTier-v2.6.4-source.zip')
  Invoke-WebRequest 'https://raw.githubusercontent.com/EasyTier/EasyTier/8428a89d2dabc94c97d370ec607c6ca142473626/LICENSE' -OutFile (Join-Path $out 'LICENSE-EasyTier')
  Invoke-WebRequest 'https://www.gnu.org/licenses/gpl-3.0.txt' -OutFile (Join-Path $out 'COPYING-GPL-3.0.txt')
  @'
EasyTier v2.6.4, unmodified official Windows x86_64 binary.
Copyright EasyTier contributors. Licensed LGPL-3.0; see LICENSE-EasyTier.
Source commit: 8428a89d2dabc94c97d370ec607c6ca142473626
Corresponding upstream source is included in EasyTier-v2.6.4-source.zip.
Upstream/build instructions: https://github.com/EasyTier/EasyTier/tree/v2.6.4
MeshClip invokes this separately replaceable executable with --config-file.
You may replace easytier-core.exe with a compatible build, including your own.
MeshClip's MIT license does not replace EasyTier or its dependencies' licenses.
Retain all upstream licenses/notices when redistributing. See upstream Cargo.lock
and source archive for dependencies and their separate license terms.
'@ | Set-Content (Join-Path $out 'NOTICE.txt') -Encoding utf8
} finally { Remove-Item -Recurse -Force $temp }
