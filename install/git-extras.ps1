$ErrorActionPreference = 'Stop'

$tmp = "$env:TEMP\git-extras-install"
$null = ni $tmp -I Directory -Force
pushd $tmp
try {
	git clone https://github.com/tj/git-extras.git .
	& .\install.cmd ((scoop prefix git).Trim())
}
finally {
	rm -Recurse -Force .\* -ea 0
	popd
}
