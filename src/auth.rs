use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use directories::BaseDirs;

/// Retrieves the standard path for saving the YouTube Music cookie:
/// ~/.config/rusty-tube/cookie.txt
pub fn get_cookie_path() -> Option<PathBuf> {
    BaseDirs::new().map(|base_dirs| {
        base_dirs
            .home_dir()
            .join(".config")
            .join("rusty-tube")
            .join("cookie.txt")
    })
}

/// Loads the cookie string from ~/.config/rusty-tube/cookie.txt
#[allow(dead_code)]
pub fn load_cookie() -> Option<String> {
    let path = get_cookie_path()?;
    if path.exists() {
        fs::read_to_string(path).ok().map(|s| s.trim().to_string())
    } else {
        None
    }
}

/// Saves the cookie string to ~/.config/rusty-tube/cookie.txt
pub fn save_cookie(cookie_content: &str) -> std::io::Result<PathBuf> {
    let path = get_cookie_path()
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "Could not find home directory"))?;
    
    // Sanitize the cookie:
    let mut content = cookie_content.trim().to_string();
    
    // Strip case-insensitive "cookie:" prefix if copied from browser developer tools headers
    if content.to_lowercase().starts_with("cookie:") {
        content = content["cookie:".len()..].trim().to_string();
    }
    
    // Ensure the cookie ends with a semicolon for reliable parsing
    if !content.ends_with(';') {
        content.push(';');
    }
    
    // Extract SAPISID from secure variants if standard SAPISID is missing.
    // The ytmapi-rs library expects "SAPISID=" to be present.
    if !content.contains("SAPISID=") {
        let secure_keys = [
            "__Secure-3PAPISID=",
            "__Secure-1PAPISID=",
            "__Secure-3SAPISID=",
            "__Secure-1SAPISID=",
        ];
        
        for key in &secure_keys {
            if let Some(idx) = content.find(key) {
                let val_part = &content[idx + key.len()..];
                if let Some(end_idx) = val_part.find(';') {
                    let val = &val_part[..end_idx];
                    content.push_str(&format!("SAPISID={};", val));
                    break;
                }
            }
        }
    }
    
    // Create the parent directory (~/.config/rusty-tube/) if it doesn't exist
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    fs::write(&path, content)?;
    Ok(path)
}

/// Opens the YouTube Music login page in the user's default browser.
/// The user signs in there, then returns to the app to import the session.
pub fn open_browser_login() -> std::io::Result<()> {
    let url = "https://music.youtube.com";

    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg(url).spawn();

    #[cfg(target_os = "linux")]
    let result = std::process::Command::new("xdg-open").arg(url).spawn();

    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();

    result.map(|_| ())
}

/// Returns true if the cookie set indicates a signed-in Google/YouTube session.
/// The YouTube Music API requires one of the *APISID variants to sign requests.
fn has_auth_cookie(map: &HashMap<String, String>) -> bool {
    map.contains_key("SAPISID")
        || map.contains_key("__Secure-3PAPISID")
        || map.contains_key("__Secure-1PAPISID")
}

/// Extracts the YouTube Music session cookies directly from an installed browser.
///
/// Tries each supported browser in turn and uses the first one that has a
/// signed-in session, returning a `name=value; ...` cookie header string.
/// This avoids the error-prone manual copy/paste of the Cookie header.
pub fn extract_browser_cookies() -> Result<String, String> {
    type BrowserFn = fn(Option<Vec<String>>) -> rookie::Result<Vec<rookie::enums::Cookie>>;
    let browsers: &[(&str, BrowserFn)] = &[
        ("Chrome", rookie::chrome),
        ("Brave", rookie::brave),
        ("Arc", rookie::arc),
        ("Edge", rookie::edge),
        ("Vivaldi", rookie::vivaldi),
        ("Opera", rookie::opera),
        ("Firefox", rookie::firefox),
        ("Safari", rookie::safari),
    ];

    let domains = Some(vec!["youtube.com".to_string()]);
    let mut found_browser = false;

    for (_name, browser_fn) in browsers {
        // A browser that isn't installed (or whose cookie DB is locked) errors out; skip it.
        let cookies = match browser_fn(domains.clone()) {
            Ok(c) if !c.is_empty() => c,
            _ => continue,
        };
        found_browser = true;

        // Collapse to a single name->value map. Within one browser the
        // youtube.com auth cookies are unique, so last-wins is fine.
        let mut map: HashMap<String, String> = HashMap::new();
        for c in cookies {
            map.insert(c.name, c.value);
        }

        if has_auth_cookie(&map) {
            let header = map
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("; ");
            return Ok(header);
        }
    }

    if found_browser {
        Err("Found a browser but no signed-in YouTube Music session. \
             Press Ctrl+O to open music.youtube.com, log in, then press Enter again."
            .to_string())
    } else {
        Err("Couldn't read cookies from any browser. Log into music.youtube.com \
             in Chrome, Brave, Arc, Edge, or Firefox, then retry."
            .to_string())
    }
}

/// Deletes the cookie file (logging out)
pub fn delete_cookie() -> std::io::Result<()> {
    if let Some(path) = get_cookie_path() {
        if path.exists() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}
