# Startup Popup Notifications
# Provides toast-style popup notifications for startup manager

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

$script:ActivePopupCount = 0

function Show-PopupNotification {
    param(
        [string]$Title,
        [string]$Message,
        [int]$Duration = 3000
    )
    try {
        $form = New-Object System.Windows.Forms.Form
        $form.FormBorderStyle = 'FixedSingle'
        $form.BackColor = [System.Drawing.Color]::FromArgb(45, 45, 48)
        $form.Width = 350
        $form.Height = 100
        $form.StartPosition = 'Manual'
        $form.TopMost = $true
        $form.ShowInTaskbar = $false
        $form.MaximizeBox = $false
        $form.MinimizeBox = $false
        $form.ControlBox = $false

        $screen = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
        $margin = 20
        $spacing = 10

        $form.Left = $screen.Right - 350 - $margin
        $form.Top = $screen.Bottom - 100 - $margin - ($script:ActivePopupCount * 110)
        $script:ActivePopupCount++

        $labelTitle = New-Object System.Windows.Forms.Label
        $labelTitle.Text = $Title
        $labelTitle.ForeColor = [System.Drawing.Color]::White
        $labelTitle.Font = New-Object System.Drawing.Font("Segoe UI", 11, [System.Drawing.FontStyle]::Bold)
        $labelTitle.AutoSize = $false
        $labelTitle.SetBounds(10, 10, 330, 30)
        $form.Controls.Add($labelTitle)

        $labelMessage = New-Object System.Windows.Forms.Label
        $labelMessage.Text = $Message
        $labelMessage.ForeColor = [System.Drawing.Color]::LightGray
        $labelMessage.Font = New-Object System.Drawing.Font("Segoe UI", 9)
        $labelMessage.AutoSize = $false
        $labelMessage.SetBounds(10, 40, 330, 50)
        $form.Controls.Add($labelMessage)

        $timer = New-Object System.Windows.Forms.Timer
        $timer.Interval = $Duration
        $timer.Add_Tick({ $form.Close(); $timer.Dispose() })
        $timer.Start()

        $form.Add_FormClosed({
            $script:ActivePopupCount = [Math]::Max(0, $script:ActivePopupCount - 1)
        })

        $form.Add_Click({ $form.Close() })
        $labelTitle.Add_Click({ $form.Close() })
        $labelMessage.Add_Click({ $form.Close() })

        $form.Show()
        [System.Windows.Forms.Application]::DoEvents()
    } catch {
        # Silently ignore popup errors
    }
}

function Show-AppStatus {
    param([string]$AppName, [string]$Status)
    switch ($Status) {
        "Starting" { Show-PopupNotification "Starting" "Launching $AppName..." 2000 }
        "Success"  { Show-PopupNotification "Success" "$AppName started" 2000 }
        "Error"    { Show-PopupNotification "Error" "$AppName failed" 3000 }
    }
}
