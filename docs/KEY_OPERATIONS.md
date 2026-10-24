# Optional advanced signature tools / 可选高级签名

默认安装和更新采用GitHub HTTPS＋SHA256，不要求发行签名、离线私钥、备份或吊销体系。此文件不是首发审批门槛。

Default installation and updates use GitHub HTTPS and SHA256. No publisher signing key is required for the standard release process.

原packaging/bootstrap-signed.sh、release-auth.sh、signed-update.sh及scripts/sign-release.py保留供明确选择固定公钥签名路径的高级维护者。该路径失败会拒绝，不自动切换到默认安装；默认路径也不会将签名错误当作成功。不要把两种信任模型混称为独立认证。

Advanced users may explicitly choose the retained fixed-key signature tools. Signature failures remain fatal on that path, with no automatic checksum-only fallback. These tools never justify uploading a private key to a repository, artifact, log or test image. Tests use disposable synthetic keys only.
