//! The per-install bearer token that authenticates every MCP request, and where it lives.
//! The token is never logged and never written into a skill (`docs/wiki/security.md`).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::AgentError;

const TOKEN_BYTES: usize = 32;

/// 32 random bytes, hex-encoded.
pub fn generate() -> Result<String, AgentError> {
    let mut bytes = [0u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(|e| AgentError::Random(e.to_string()))?;
    Ok(hex(&bytes))
}

/// A cryptographically random session id of visible ASCII (hex).
pub fn session_id() -> Result<String, AgentError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|e| AgentError::Random(e.to_string()))?;
    Ok(hex(&bytes))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Compares without stopping at the first difference, so timing does not reveal a prefix.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        diff |= usize::from(x ^ y);
    }
    diff == 0
}

/// The per-user directory for OpenDesigner's own state. `OPENDESIGNER_CONFIG_DIR` overrides it.
pub fn config_dir() -> Result<PathBuf, AgentError> {
    if let Some(dir) = std::env::var_os("OPENDESIGNER_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let env = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let dir = if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support/OpenDesigner"))
    } else if cfg!(target_os = "windows") {
        env("APPDATA").map(|a| a.join("OpenDesigner"))
    } else {
        env("XDG_CONFIG_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".config")))
            .map(|c| c.join("opendesigner"))
    };
    dir.ok_or_else(|| AgentError::Config("cannot find a per-user config directory".into()))
}

pub fn token_path() -> Result<PathBuf, AgentError> {
    Ok(config_dir()?.join("mcp-token"))
}

/// Reads the token at `path`, creating it (readable by the user only) when it does not exist.
pub fn load_or_create(path: &Path) -> Result<String, AgentError> {
    match fs::read_to_string(path) {
        Ok(t) if t.trim().len() == TOKEN_BYTES * 2 => return Ok(t.trim().to_string()),
        Ok(_) | Err(_) => {}
    }
    write_new(path)
}

/// Replaces the token. Agents configured with the old one must be reconnected.
pub fn rotate(path: &Path) -> Result<String, AgentError> {
    write_new(path)
}

fn write_new(path: &Path) -> Result<String, AgentError> {
    let token = generate()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| AgentError::io(dir, e))?;
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| AgentError::io(path, e))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|e| AgentError::io(path, e))?;
    }
    file.write_all(token.as_bytes())
        .map_err(|e| AgentError::io(path, e))?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_hex_of_the_right_length() {
        let a = generate().unwrap();
        let b = generate().unwrap();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
        assert!(session_id().unwrap().len() == 32);
    }

    #[test]
    fn constant_time_eq_matches_equality() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"a"));
        assert!(constant_time_eq(b"", b""));
    }

    #[test]
    fn the_token_file_is_created_once_private_and_rotates() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("cfg/mcp-token");
        let first = load_or_create(&path).unwrap();
        assert_eq!(load_or_create(&path).unwrap(), first);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let second = rotate(&path).unwrap();
        assert_ne!(second, first);
        assert_eq!(load_or_create(&path).unwrap(), second);
    }
}
