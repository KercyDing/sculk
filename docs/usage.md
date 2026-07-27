# 使用说明

## 建房

```sh
sculk host --port 25565 --max-players 8
```

成功后会输出一条可分享的邀请链接：

```text
sculk://join/v1/<payload>
```

把完整链接发给想一起玩的朋友即可。请不要把它发到公开场合。默认情况下，
停止房间后再次开启时会生成新链接。

### 分享链接刷新策略

使用 `--time`（简写为 `-t`）修改刷新策略：

```sh
# 每次开启房间都生成新链接（默认）
sculk host --time always

# 一直复用当前链接
sculk host --time never

# 每 3 小时生成新链接
sculk host --time 3h
```

可选值为 `always`、`never`、`1h`、`3h`、`6h`、`12h` 和 `24h`。显式指定
`--time` 后，选择会保存到配置中；以后直接运行 `sculk host` 会沿用该策略。
时间策略从当前链接的创建时间开始计算，关闭程序不会暂停计时。

需要保留当前策略、但仅在这次开启房间时生成新链接，可使用：

```sh
sculk host --force
```

`--force`（简写为 `-f`）不会生成新的联机身份，也不会修改已保存的刷新策略。
它可以和 `--time` 同时使用，例如：

```sh
sculk host --time never --force
```

这会立即生成新链接，并在以后继续复用该链接，直到再次强制刷新或修改策略。

## 加入

```sh
sculk join "sculk://join/v1/<payload>"
```

默认会自动选择可用端口。只有需要固定端口时才使用：

```sh
sculk join "sculk://join/v1/<payload>" --port 30000
```

连接成功后 CLI 会显示一个本地地址；在 Minecraft 的“多人游戏”中添加该地址即可进入。

## Relay

```sh
sculk relay --list
sculk relay --url https://your-relay.example.com
sculk relay --reset
```

通常不需要修改 Relay。若修改，当前开启的房间会短暂断开，需要重新开启并分享新链接。

## 分享链接

- `always` 策略下，停止或重新开启房间后，旧链接不能再使用。
- `never` 和未到期的时间策略会在重新开启房间后继续使用当前链接。
- 使用 `--force` 或等待自动刷新后，请把新链接重新发给尚未加入的朋友。
- 已经连接并正在游玩的玩家不会因为刷新链接而掉线。
- 不要删除密钥文件；删除后会变成一个全新的联机身份，原有链接也无法继续使用。
