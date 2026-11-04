# ELYTerminal: wrap the prompt to report folder and title and to mark prompts and commands.
# A Claude tab's id and prelude stay the shell's own, for its first prompt.
$global:ElyClaude = $env:ELY_CLAUDE
$global:ElyClaudePrelude = $env:ELY_CLAUDE_PRELUDE
Remove-Item Env:ELY_CLAUDE, Env:ELY_CLAUDE_PRELUDE -ErrorAction Ignore
$global:ElyPrompt = $function:prompt
$global:ElyRan = $false
$global:ElyCmdBack = $null
function global:prompt {
    # First: every statement sets $? again.
    $ok = $?
    ElyCmdBack
    if ($global:ElyClaude) { ElyClaude }
    $esc, $bel = [char]27, [char]7
    if ($global:ElyRan) {
        $code = 0
        if (-not $ok) {
            # Each native run sets a new $LASTEXITCODE, equal or not; strict mode refuses an unset one.
            $exit = Get-Variable LASTEXITCODE -Scope Global -ValueOnly -ErrorAction Ignore
            $ran = $null -ne $exit -and -not [object]::ReferenceEquals($exit, $global:ElyExit)
            $code = if (-not $global:ElyShell -and $ran -and $exit -ne 0) { $exit } else { 1 }
        }
        [Console]::Write("$esc]133;D;$code$bel")
        $global:ElyRan = $false
    }
    [Console]::Write("$esc]133;A;aid=elyterminal$bel")
    # The user's prompt sees a failed command's $? still.
    if (-not $ok) { Write-Error '' -ErrorAction Ignore }
    $text = & $global:ElyPrompt
    $folder = $executionContext.SessionState.Path.CurrentLocation
    if ($folder.Provider.Name -eq 'FileSystem') {
        $parts = $folder.ProviderPath.Replace('\', '/') -split '/'
        $path = ($parts | ForEach-Object { [Uri]::EscapeDataString($_) }) -join '/'
        if (-not $path.StartsWith('/')) { $path = "/$path" }
        $leaf = (Split-Path -Leaf $folder.ProviderPath) -replace '\p{Cc}', ''
        [Console]::Write("$esc]7;file://$path$bel$esc]2;$leaf$bel")
    }
    "$text$esc]133;B$bel"
}
# Claude as a marked command, once: no eval of its id.
function global:ElyClaude {
    $ask, $prelude = $global:ElyClaude, $global:ElyClaudePrelude
    $global:ElyClaude, $global:ElyClaudePrelude = $null, $null
    $esc, $bel = [char]27, [char]7
    $word = if ($ask -clike 'new:*') { '--session-id' } elseif ($ask -clike 'resume:*') { '--resume' }
    $id = $ask -creplace '^[^:]*:', ''
    if (-not $word -or $id -cnotmatch '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$') {
        [Console]::Error.WriteLine('ELYTerminal: ELY_CLAUDE names no Claude session, so claude does not start')
        return
    }
    [Console]::Write("$esc]133;A;aid=elyterminal$bel$esc]133;C$bel$esc]2;claude$bel")
    if ($prelude) {
        try { Invoke-Expression $prelude } catch { [Console]::Error.WriteLine("ELYTerminal: the Claude prelude failed: $_") }
    }
    $global:LASTEXITCODE = $null
    $code = try {
        & claude $word $id
        if ($?) { 0 } elseif ($global:LASTEXITCODE) { $global:LASTEXITCODE } else { 1 }
    } catch {
        [Console]::Error.WriteLine("ELYTerminal: claude did not start: $_")
        127
    }
    [Console]::Write("$esc]133;D;$code$bel")
}
# Git Bash's /e/x is E:\x where nothing lies at \e\x here.
function global:ElyDrivePath([string]$line) {
    # One line, one plain token: no quote of any kind.
    $token = '^([ \t]*cd[ \t]+)/([A-Za-z])(/[\w.\-/*?\[\]+@#%!,=~]*)?([ \t]*)$'
    if ($env:OS -ne 'Windows_NT' -or $line -match '[\r\n]' -or $line -notmatch $token) { return $line }
    $lead, $letter, $rest, $tail = $Matches[1], $Matches[2], "$($Matches[3])", $Matches[4]
    $cd = Get-Command cd -ErrorAction Ignore
    $own = -not ($cd -is [System.Management.Automation.AliasInfo] -and
        $cd.ResolvedCommand -is [System.Management.Automation.CmdletInfo] -and
        $cd.ResolvedCommand.Name -eq 'Set-Location')
    # A cd of the user's own, or a path that exists, stays as typed.
    if ($own -or (Test-Path -Path "/$letter$rest" -ErrorAction Ignore) -or
        -not (Test-Path -LiteralPath "${letter}:\" -ErrorAction Ignore)) { return $line }
    "$lead$($letter.ToUpperInvariant()):\$($rest.TrimStart('/').Replace('/', '\'))$tail"
}
# A bare cmd typed here starts with ELYTerminal's marked prompt.
function global:ElyCmd([string]$line) {
    if ($env:OS -ne 'Windows_NT' -or -not $env:ELY_CMD_PROMPT -or $line -notmatch '^[ \t]*(cmd(\.exe)?)[ \t]*$') { return $line }
    # A cmd of the user's own stays as typed.
    if ((Get-Command $Matches[1] -ErrorAction Ignore) -isnot [System.Management.Automation.ApplicationInfo]) { return $line }
    # Through the environment, unparsed; the typed line keeps its status.
    $global:ElyCmdBack = @{ Prompt = $env:PROMPT }
    $env:PROMPT = $env:ELY_CMD_PROMPT
    $line
}
# The next prompt gives PowerShell its own PROMPT back.
function global:ElyCmdBack {
    if ($global:ElyCmdBack) {
        $env:PROMPT = $global:ElyCmdBack.Prompt
        $global:ElyCmdBack = $null
    }
}
# A command starts once PSReadLine hands over a line that is not blank.
if (Test-Path Function:\PSConsoleHostReadLine) {
    $global:ElyReadLine = $function:PSConsoleHostReadLine
    function global:PSConsoleHostReadLine {
        $line = ElyCmd (ElyDrivePath (& $global:ElyReadLine))
        if (-not [string]::IsNullOrWhiteSpace($line)) {
            $global:ElyRan = $true
            $global:ElyExit = Get-Variable LASTEXITCODE -Scope Global -ValueOnly -ErrorAction Ignore
            # A program's code is the line's, unless the line surely ends in PowerShell: a cmdlet, a name not found, or no command.
            $ast = [System.Management.Automation.Language.Parser]::ParseInput($line, [ref]$null, [ref]$null)
            # Strict mode refuses .Count on one statement alone, so the statements stay a list.
            $statements = @(if ($ast.EndBlock) { $ast.EndBlock.Statements })
            $last = if ($statements.Count) { $statements[$statements.Count - 1] }
            if ($last -is [System.Management.Automation.Language.PipelineAst]) {
                $elements = $last.PipelineElements
                $last = $elements[$elements.Count - 1]
            }
            $name = if ($last -is [System.Management.Automation.Language.CommandAst]) { $last.GetCommandName() }
            $global:ElyShell = if ($name) {
                $pattern = [System.Management.Automation.WildcardPattern]::Escape($name)
                $info = Get-Command -Name $pattern -ErrorAction Ignore | Select-Object -First 1
                while ($info -and $info.CommandType -eq 'Alias') { $info = $info.ResolvedCommand }
                $null -eq $info -or $info.CommandType -eq 'Cmdlet'
            } else {
                $null -eq $last -or -not $last.Find({ $args[0] -is [System.Management.Automation.Language.CommandAst] }, $true)
            }
            [Console]::Write("$([char]27)]133;C$([char]7)")
        }
        $line
    }
} else {
    [Console]::Error.WriteLine('ELYTerminal: PowerShell marks no command without PSReadLine')
}
