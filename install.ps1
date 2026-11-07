# Installs Lattice on Windows from its latest GitHub release, in PowerShell:
#
#     irm https://raw.githubusercontent.com/mochreach/lattice/main/install.ps1 | iex
#
# It checks the download's checksum, puts lattice.exe and the example songs in
# %LOCALAPPDATA%\Programs\lattice, and adds that folder to your PATH. Nothing needs to run as
# administrator. Settings, all optional: $env:LATTICE_VERSION (a release tag, such as v0.1.0,
# rather than the latest) and $env:LATTICE_INSTALL_DIR (rather than the folder above).
# Running it again updates Lattice.

# All in a function, called on the last line, so a download cut short runs nothing.
function Install-Lattice {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue' # Invoke-WebRequest is far slower drawing progress
    $repo = 'mochreach/lattice'
    $base = if ($env:LATTICE_BASE_URL) { $env:LATTICE_BASE_URL } else { "https://github.com/$repo/releases" }
    $version = if ($env:LATTICE_VERSION) { $env:LATTICE_VERSION } else { 'latest' }
    $dir = if ($env:LATTICE_INSTALL_DIR) { $env:LATTICE_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\lattice' }

    # Only an x86_64 build; Windows on ARM runs it emulated.
    $build = 'x86_64-windows'
    $url = if ($version -eq 'latest') {
        "$base/latest/download/lattice-$build.zip"
    } else {
        "$base/download/$version/lattice-$build.zip"
    }

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("lattice-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        Write-Host "Downloading Lattice ($build)..."
        $zip = Join-Path $tmp 'lattice.zip'
        Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $zip
        Invoke-WebRequest -UseBasicParsing -Uri "$url.sha256" -OutFile "$zip.sha256"
        $want = ((Get-Content "$zip.sha256" -Raw) -split '\s+')[0].ToLower()
        $got = (Get-FileHash -Algorithm SHA256 $zip).Hash.ToLower()
        if ($want -ne $got) { throw "the download doesn't match its checksum: try again" }
        Expand-Archive -Path $zip -DestinationPath $tmp
        $unpacked = Join-Path $tmp "lattice-$build"

        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        Copy-Item (Join-Path $unpacked 'lattice.exe') (Join-Path $dir 'lattice.exe') -Force
        $examples = Join-Path $dir 'examples'
        if (Test-Path $examples) { Remove-Item -Recurse -Force $examples }
        Copy-Item -Recurse (Join-Path $unpacked 'examples') $examples
    } finally {
        Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
    }

    $path = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (-not (($path -split ';') -contains $dir)) {
        $joined = if ($path) { "$path;$dir" } else { $dir }
        [Environment]::SetEnvironmentVariable('Path', $joined, 'User')
        $env:Path = "$env:Path;$dir"
        Write-Host "Added $dir to your PATH; terminals opened from now on will find lattice."
    }

    Write-Host ""
    Write-Host "Installed $(& (Join-Path $dir 'lattice.exe') --version) in $dir"
    Write-Host "The example songs are in $examples"
    Write-Host ""
    Write-Host "Lattice on Windows is untried, and goes without lattice ctl, recording another app's"
    Write-Host "sound, and a MIDI port of its own. It wants a terminal that reports every key, such"
    Write-Host "as WezTerm with enable_kitty_keyboard = true."
    Write-Host ""
    Write-Host "Start with the tutor:     lattice --tutor"
    Write-Host "Or open an example:       lattice $examples\amen\amen.lattice"
    Write-Host "If you're stuck:          lattice --help"
}

Install-Lattice
