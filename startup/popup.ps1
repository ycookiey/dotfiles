$script:ActivePopupCount = 0
# Stack slots before wrapping back to the bottom. dotcli notify stacks each slot 110px above the
# last, so slot N would sit above the top of the screen on a 1080p display from N = 9.
$script:PopupSlots = 5

function Show-PopupNotification {
    param([string]$Title, [string]$Message, [int]$Duration = 3000)
    $offset = $script:ActivePopupCount % $script:PopupSlots
    $script:ActivePopupCount++
    # Start-Process joins -ArgumentList with spaces without quoting, which split a multi-word title
    # or message into extra arguments. Quote them here; '"' inside becomes "'" so it cannot unquote.
    $t = $Title -replace '"', "'"
    $m = $Message -replace '"', "'"
    Start-Process -FilePath "dotcli" -ArgumentList "notify -t `"$t`" -m `"$m`" -d $Duration -o $offset" -WindowStyle Hidden
}

function Show-AppStatus {
    param([string]$AppName, [string]$Status)
    switch ($Status) {
        "Error" { Show-PopupNotification "Error" "$AppName failed" 3000 }
    }
}
