$ErrorActionPreference = 'Stop'

Push-Location (Join-Path $PSScriptRoot 'temp')
try {
	git clone https://github.com/tj/git-extras.git .
	& .\install.cmd ((scoop prefix git).Trim())
}
finally {
	Remove-Item -Recurse -Force .\* -ErrorAction SilentlyContinue
	Pop-Location
}