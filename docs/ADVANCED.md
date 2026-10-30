# Advanced maintenance / 高级维护

普通安装由后台自动推进；以下既有命令用于人工维护。历史不确定的账号不要使用 FRESH。当前菜单路径见首页。

```sh
sudo kairo recover wizard
```

```sh
sudo kairo recover begin --acknowledge-unknown-history
sudo kairo recover status
```

```sh
sudo kairo recover check
```

```sh
sudo kairo enable --approve-production-cutover
sudo kairo status
sudo kairo doctor
```

```sh
sudo kairo schedule list
sudo kairo schedule add 02:00
sudo kairo schedule edit 02:00 03:30
sudo kairo schedule remove 03:30
sudo kairo schedule next
sudo kairo schedule preview
```

```sh
sudo kairo status
sudo kairo quota
sudo kairo logs
```

```sh
sudo kairo update online
```

[Automatic initialization / 自动初始化](AUTOMATIC_INITIALIZATION.md)
