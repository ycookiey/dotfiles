$script:ActivePopupCount = 0

function Show-PopupNotification {
    param([string]$Title, [string]$Message, [int]$Duration = 3000)
    $offset = $script:ActivePopupCount
    $script:ActivePopupCount++
    Start-Process -FilePath "dotcli" -ArgumentList "notify", "-t", $Title, "-m", $Message, "-d", $Duration, "-o", $offset -WindowStyle Hidden
}

function Show-AppStatus {
    param([string]$AppName, [string]$Status)
    switch ($Status) {
        "Starting" { Show-PopupNotification "Starting" "Launching $AppName..." 2000 }
        "Success"  { Show-PopupNotification "Success" "$AppName started" 2000 }
        "Error"    { Show-PopupNotification "Error" "$AppName failed" 3000 }
    }
}
