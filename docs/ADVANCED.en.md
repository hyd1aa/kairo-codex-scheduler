# Advanced maintenance / 高级维护

Ordinary setup is automatic; these existing commands remain available for manual troubleshooting. Do not use FRESH for accounts with uncertain history. Current menu paths are in the homepage.

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
