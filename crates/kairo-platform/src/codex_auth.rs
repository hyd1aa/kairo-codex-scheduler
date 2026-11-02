//! Ask the official CLI to renew its own managed OAuth. Never starts a thread/turn.
use crate::{runtime::Runtime, Store};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::process::CommandExt,
    process::{Child, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
struct Session(Child);
impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-(self.0.id() as i32), libc::SIGKILL);
        }
        let _ = self.0.wait();
    }
}
pub fn renew(store: &Store, r: &Runtime) -> Result<()> {
    if std::env::var_os("KAIRO_OFFLINE").as_deref() == Some(std::ffi::OsStr::new("1")) {
        bail!("offline_mode / 离线模式禁止续期");
    }
    protocol(store, r)
}
fn protocol(store: &Store, r: &Runtime) -> Result<()> {
    let mut command = crate::management::account_command(store, &r.codex)?;
    let child = command
        .args(["app-server", "--listen", "stdio://"])
        .env_clear()
        .env("HOME", &r.home)
        .env("CODEX_HOME", &r.codex_home)
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("LANG", "C.UTF-8")
        .process_group(0)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut session = Session(child);
    let _watch = crate::guardian::ParentWatch::new(session.0.id())?;
    let mut input = session.0.stdin.take().context("oauth_cli_protocol")?;
    let output = session.0.stdout.take().context("oauth_cli_protocol")?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(output.take(1024 * 1024)).lines() {
            let parsed = line
                .ok()
                .and_then(|line| serde_json::from_str::<Value>(&line).ok());
            if tx.send(parsed).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    let receive = |id: u64| -> Result<Value> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                bail!("oauth_refresh_timeout / 官方凭证续期超时");
            }
            let value = rx
                .recv_timeout(remaining)
                .ok()
                .flatten()
                .context("oauth_refresh_failed / 官方凭证续期失败")?;
            if value["id"] == id {
                if value.get("error").is_some() {
                    bail!("oauth_refresh_failed / 官方凭证续期失败");
                }
                return Ok(value["result"].clone());
            }
        }
    };
    writeln!(
        input,
        "{}",
        json!({"id":0,"method":"initialize","params":{"clientInfo":{"name":"kairo","title":"Kairo","version":env!("CARGO_PKG_VERSION")}}})
    )?;
    input.flush()?;
    receive(0)?;
    writeln!(input, "{}", json!({"method":"initialized"}))?;
    writeln!(
        input,
        "{}",
        json!({"id":1,"method":"account/read","params":{"refreshToken":true}})
    )?;
    input.flush()?;
    let account = receive(1)?;
    if account["account"]["type"] != "chatgpt" {
        bail!("chatgpt_oauth_required / 需要官方 ChatGPT OAuth 登录");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mocked_official_refresh_only_sends_initialize_and_account_read() {
        let path = std::env::temp_dir().join(format!("kairo-auth-renew-{}", std::process::id()));
        let store = Store::open(&path).unwrap();
        let script = path.join("codex");
        std::fs::write(
            &script,
            r#"#!/usr/bin/python3
import json,sys
assert sys.argv[1:]==['app-server','--listen','stdio://']
for expected in ['initialize','initialized','account/read']:
 v=json.loads(sys.stdin.readline());assert v['method']==expected
 if expected=='account/read':
  assert v['params']=={'refreshToken':True}
  print(json.dumps({'id':v['id'],'result':{'account':{'type':'chatgpt'}}}),flush=True)
 elif expected=='initialize':print(json.dumps({'id':v['id'],'result':{}}),flush=True)
"#,
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let r = Runtime {
            codex: script,
            home: path.clone(),
            codex_home: path.clone(),
            ..Default::default()
        };
        protocol(&store, &r).unwrap();
        assert!(!path.join("auth.json").exists());
        drop(store);
        std::fs::remove_dir_all(path).unwrap();
    }
}
