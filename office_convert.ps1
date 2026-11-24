$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$source = $env:SIMPLEPDF_OFFICE_INPUT
$target = $env:SIMPLEPDF_OFFICE_OUTPUT
$app = $null
$item = $null
$previousSecurity = $null
try {
    switch -Regex ([IO.Path]::GetExtension($source)) {
        '^\.(docx?|rtf|odt)$' {
            $app = New-Object -ComObject Word.Application
            $app.AutomationSecurity = 3
            $app.Visible = $false
            $app.DisplayAlerts = 0
            $item = $app.Documents.Open($source,$false,$true,$false)
            $item.ExportAsFixedFormat($target,17)
        }
        '^\.(xlsx?|xlsm|csv)$' {
            $app = New-Object -ComObject Excel.Application
            $app.AutomationSecurity = 3
            $app.Visible = $false
            $app.DisplayAlerts = $false
            $item = $app.Workbooks.Open($source,0,$true)
            $item.ExportAsFixedFormat(0,$target)
        }
        '^\.pptx?$' {
            $app = New-Object -ComObject PowerPoint.Application
            $previousSecurity = $app.AutomationSecurity
            $app.AutomationSecurity = 3
            $item = $app.Presentations.Open($source,-1,-1,0)
            $item.SaveAs($target,32)
        }
        default { throw 'Unsupported Office format' }
    }
} finally {
    if ($null -ne $item) {
        try { if ($source -match '\.pptx?$') {$item.Close()} else {$item.Close($false)} } finally { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($item) }
    }
    if ($null -ne $app) {
        try {
            if ($null -ne $previousSecurity) {
                $app.AutomationSecurity = $previousSecurity
                if ($app.Presentations.Count -eq 0) { $app.Quit() }
            } else {
                $app.Quit()
            }
        } finally { [void][Runtime.InteropServices.Marshal]::FinalReleaseComObject($app) }
    }
}
