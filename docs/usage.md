# 使用说明

## 建房

```sh
sckc host --port 25565 --max-players 8
```

成功后会输出一条可分享的邀请链接：

```text
sculk://join/v1/<payload>
```

把完整链接发给想一起玩的朋友即可。请不要把它发到公开场合；停止房间后再次开启时，会生成新链接。

## 加入

```sh
sckc join "sculk://join/v1/<payload>"
```

默认会自动选择可用端口。只有需要固定端口时才使用：

```sh
sckc join "sculk://join/v1/<payload>" --port 30000
```

连接成功后 CLI 会显示一个本地地址；在 Minecraft 的“多人游戏”中添加该地址即可进入。

## Relay

```sh
sckc relay --list
sckc relay --url https://your-relay.example.com
sckc relay --reset
```

通常不需要修改 Relay。若修改，当前开启的房间会短暂断开，需要重新开启并分享新链接。

## 分享链接

- 链接只在当前房间开启期间有效；停止或重新开启房间后，旧链接不能再使用。
- 手动刷新链接或等待自动刷新后，请把新链接重新发给尚未加入的朋友。
- 已经连接并正在游玩的玩家不会因为刷新链接而掉线。
- 不要删除密钥文件；删除后会变成一个全新的联机身份，原有链接也无法继续使用。
