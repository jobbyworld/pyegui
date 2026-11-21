# Hides entries of the Explorer context menu that live in HKLM (run it in an administrator PowerShell).
# Reversible: "-Undo" removes the LegacyDisable value again. Nothing is deleted.
param([switch]$Undo)
$verbs = 'git_gui', 'git_shell', 'AddToPlaylistVLC', 'PlayWithVLC', 'Sublime Text'
$count = 0
foreach ($verb in $verbs) {
    # reg.exe finds the keys in seconds, walking the registry from PowerShell takes minutes
    $found = reg query 'HKLM\Software\Classes' /s /f $verb /k 2>$null |
        Where-Object { $_ -like "*\shell\$verb" -and $_ -notlike '*\command' }
    foreach ($key in $found) {
        if ($Undo) { reg delete $key /v LegacyDisable /f 2>&1 | Out-Null }
        else { reg add $key /v LegacyDisable /t REG_SZ /d '' /f | Out-Null }
        $count++
    }
}
Write-Host ("{0} entrées {1}" -f $count, $(if ($Undo) { 'rétablies' } else { 'masquées' }))
