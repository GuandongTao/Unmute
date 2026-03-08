use std::process::Command;
use std::thread;
use std::time::Duration;

/// Paste text into the currently focused application.
/// Strategy: save clipboard → set text → simulate Ctrl+V → restore clipboard
pub fn paste_text(text: &str) -> Result<(), String> {
    // Save entire clipboard (including images/files) to a temp variable via PowerShell
    // We use a script that detects the clipboard format and saves accordingly
    let save_script = r#"
$script:savedClip = $null
$script:savedFormat = 'none'
if ([System.Windows.Forms.Clipboard]::ContainsImage()) {
    $script:savedClip = [System.Windows.Forms.Clipboard]::GetImage()
    $script:savedFormat = 'image'
} elseif ([System.Windows.Forms.Clipboard]::ContainsFileDropList()) {
    $script:savedClip = [System.Windows.Forms.Clipboard]::GetFileDropList()
    $script:savedFormat = 'files'
} elseif ([System.Windows.Forms.Clipboard]::ContainsText()) {
    $script:savedClip = [System.Windows.Forms.Clipboard]::GetText()
    $script:savedFormat = 'text'
}
"#;

    // Build a complete script that saves, pastes, and restores
    let escaped = text.replace('\'', "''");
    let full_script = format!(
        r#"
Add-Type -AssemblyName System.Windows.Forms

# Save current clipboard
$savedClip = $null
$savedFormat = 'none'
if ([System.Windows.Forms.Clipboard]::ContainsImage()) {{
    $savedClip = [System.Windows.Forms.Clipboard]::GetImage()
    $savedFormat = 'image'
}} elseif ([System.Windows.Forms.Clipboard]::ContainsFileDropList()) {{
    $savedClip = [System.Windows.Forms.Clipboard]::GetFileDropList()
    $savedFormat = 'files'
}} elseif ([System.Windows.Forms.Clipboard]::ContainsText()) {{
    $savedClip = [System.Windows.Forms.Clipboard]::GetText()
    $savedFormat = 'text'
}}

# Set our text and paste
[System.Windows.Forms.Clipboard]::SetText('{text}')
Start-Sleep -Milliseconds 50
[System.Windows.Forms.SendKeys]::SendWait('^v')
Start-Sleep -Milliseconds 200

# Restore previous clipboard content
if ($savedFormat -eq 'image' -and $savedClip -ne $null) {{
    [System.Windows.Forms.Clipboard]::SetImage($savedClip)
}} elseif ($savedFormat -eq 'files' -and $savedClip -ne $null) {{
    [System.Windows.Forms.Clipboard]::SetFileDropList($savedClip)
}} elseif ($savedFormat -eq 'text' -and $savedClip -ne $null) {{
    [System.Windows.Forms.Clipboard]::SetText($savedClip)
}} else {{
    [System.Windows.Forms.Clipboard]::Clear()
}}
"#,
        text = escaped
    );

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", &full_script])
        .output()
        .map_err(|e| format!("Failed to paste: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Paste failed: {}", stderr));
    }

    Ok(())
}
