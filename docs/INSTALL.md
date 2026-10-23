# Installation, OAuth and maintenance / 安装、授权与维护

No public download endpoint has been validated. Binary users do not need Rust/Cargo. Linux, supported architecture, systemd, configured timezone and trusted package repositories are required; initial bootstrap needs curl, OpenSSL and sha256sum. The verified installer prepares other supported distribution dependencies and creates a dedicated user/global command/unit. Installation leaves the service disabled, stopped and observation-only.

## First install / 首次安装

Obtain the exact bootstrap SHA256 through an independently authenticated owner channel. BASE_URL must be the actual approved HTTPS Release asset base, not an example claimed as live. In a shell that stops on failures:

```sh
set -eu
curl --fail --location --proto '=https' --proto-redir '=https' --tlsv1.2 "$BASE_URL/bootstrap-signed.sh" -o bootstrap-signed.sh
printf '%s  bootstrap-signed.sh\n' "$BOOTSTRAP_SHA256" | sha256sum -c -
sudo sh ./bootstrap-signed.sh 0.2.1 --url "$BASE_URL"
```

Do not use unverified curl-pipe-shell. For an independently authenticated offline handoff, use the verified bootstrap with `0.2.1 --source-dir RELEASE_DIRECTORY`. `kairo` opens the menu; `--lang en-US`/`--lang zh-CN` selects language. The bootstrap's public key verifies the signed manifest and asset bytes before installing. 同源下载脚本、校验值和公钥本身不能建立首次信任。

## OAuth / 人工授权

Kairo does not bundle the official Codex CLI. Install it from its official distribution and verify the executable path. Keep the service stopped. Under the dedicated user:

```sh
sudo -u kairo -H kairo runtime
# Only when necessary, set the actual absolute CLI path:
sudo -u kairo -H kairo runtime --codex /ACTUAL/ABSOLUTE/codex
sudo -u kairo -H kairo auth login
sudo -u kairo -H kairo doctor
kairo status
systemctl status kairo.service
```

The user completes browser/MFA manually. Verify service-user home, credential storage compatibility and the intended test identity without logging credentials. A keyring-only credential mode may be incompatible with an auth.json reader; fail rather than copying another installation's credentials. Read-only `auth verify` needs separate authorization for network access; no model request is part of acceptance. Do not bind or activate live scheduling as a login test. Tests use synthetic credentials only; production credentials/journals must never be copied into a test workspace.

## Upgrade / 升级

Download the signed manifest, detached signature and architecture binary into a staging directory. Use the already trusted installed updater:

```sh
sudo systemctl stop kairo.service
sudo sh /opt/kairo/trust/signed-update.sh 0.2.1 RELEASE_DIRECTORY /opt/kairo /var/lib/kairo
```

Do not replace it with an unauthenticated downloaded script. Signature/hash/version errors refuse the new binary; durable transactions recover or restore old installation state. The daemon lease must be free. `kairo update rollback --prefix /opt/kairo` switches a compatible binary while retaining current request history. Review the result before deciding to resume observation; upgrade/rollback does not approve real requests. Legacy checksum-only commands are low-level administrator tools, requiring independent trusted hashes.

## Uninstall / 卸载

Run `sudo sh packaging/system-uninstall.sh` from a retained verified package. Default removal stops/removes the unit and global command while preserving data. Inspect supported flags before intentionally deleting data. Reinstall using the trusted bootstrap, preserving deliberate request-history recovery. Never remove account history merely to bypass safety guards.
