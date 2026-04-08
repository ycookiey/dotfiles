Add-Type -AN System.Windows.Forms
Add-Type -AN System.Drawing

$script:ActivePopupCount = 0

function Show-PopupNotification {
    param([string]$Title, [string]$Message, [int]$Duration = 3000)
    $offset = $script:ActivePopupCount * 110
    $script:ActivePopupCount++
    try {
        $rs = [runspacefactory]::CreateRunspace()
        $rs.ApartmentState = 'STA'
        $rs.Open()
        $ps = [powershell]::Create()
        $ps.Runspace = $rs
        [void]$ps.AddScript({
            param($t, $m, $d, $o)
            Add-Type -AN System.Windows.Forms
            Add-Type -AN System.Drawing
            Add-Type -MemberDefinition '[DllImport("user32.dll")] public static extern IntPtr FindWindow(string c, string w);' -Name W32 -Namespace Popup
            while ([Popup.W32]::FindWindow("Shell_TrayWnd", $null) -eq [IntPtr]::Zero) { [Threading.Thread]::Sleep(500) }
            $f = [Windows.Forms.Form]::new()
            $f.FormBorderStyle = 'FixedSingle'
            $f.BackColor = [Drawing.Color]::FromArgb(45, 45, 48)
            $f.Width = 350; $f.Height = 100
            $f.StartPosition = 'Manual'; $f.TopMost = $true
            $f.ShowInTaskbar = $false
            $f.MaximizeBox = $false; $f.MinimizeBox = $false; $f.ControlBox = $false
            $scr = [Windows.Forms.Screen]::PrimaryScreen.WorkingArea
            $f.Left = $scr.Right - 370; $f.Top = $scr.Bottom - 120 - $o
            $lt = [Windows.Forms.Label]::new()
            $lt.Text = $t; $lt.ForeColor = [Drawing.Color]::White
            $lt.Font = [Drawing.Font]::new("Segoe UI", 11, [Drawing.FontStyle]::Bold)
            $lt.SetBounds(10, 10, 330, 30); $f.Controls.Add($lt)
            $lm = [Windows.Forms.Label]::new()
            $lm.Text = $m; $lm.ForeColor = [Drawing.Color]::LightGray
            $lm.Font = [Drawing.Font]::new("Segoe UI", 9)
            $lm.SetBounds(10, 40, 330, 50); $f.Controls.Add($lm)
            $tmr = [Windows.Forms.Timer]::new()
            $tmr.Interval = $d
            $cl = { $tmr.Stop(); try { $f.Close() } catch {} }
            $tmr.Add_Tick($cl)
            $tmr.Start()
            $f.Add_Click($cl); $lt.Add_Click($cl); $lm.Add_Click($cl)
            [void]$f.ShowDialog()
            $tmr.Dispose(); $f.Dispose()
        }).AddArgument($Title).AddArgument($Message).AddArgument($Duration).AddArgument($offset)
        $msgData = @{ Runspace = $rs; Handle = $null }
        $msgData.Handle = $ps.BeginInvoke()
        Register-ObjectEvent $ps -EventName InvocationStateChanged -MessageData $msgData -Action {
            if ($Sender.InvocationStateInfo.State -in 'Completed','Stopped','Failed') {
                try { $Sender.EndInvoke($Event.MessageData.Handle) } catch {}
                $Sender.Dispose()
                $Event.MessageData.Runspace.Dispose()
                Unregister-Event $EventSubscriber.SourceIdentifier
            }
        } | Out-Null
    } catch {}
}

function Show-AppStatus {
    param([string]$AppName, [string]$Status)
    switch ($Status) {
        "Starting" { Show-PopupNotification "Starting" "Launching $AppName..." 2000 }
        "Success"  { Show-PopupNotification "Success" "$AppName started" 2000 }
        "Error"    { Show-PopupNotification "Error" "$AppName failed" 3000 }
    }
}
