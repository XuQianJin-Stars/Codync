# 自建香港私有中继 — 开发设计

Status: **阶段 A–C 已实现，阶段 D 未做**。2026-10-23 按当前源码落地：`private-relay/`、host `setCloudUrl`、三端把非官方 cloud 称为私人中继。账号、webhook、TURN、自建推播仍不做。

适用：个人自己的电脑，可以是多台。每台一个 host、一份 `identity.json`、一个 `computerId`，外网可以共用同一台香港 VPS。局域网 / Tailscale / 官方 Cloudflare 云继续可用。线协议以 [远程中继协议](reference/remote-relay.md) 为准。实现时若协议文档与源码不一致，以源码和协议文末「Implementation clarifications」为准。

原稿把通路写成 SSE 房间、Hono + Redis 哑转发，并打算在 host 里加一套 `RelayMode`。那套模型和现有实现不是同一条协议。按原稿改，手机会连不上，加密、配对、离线信箱、ACL 都要重写，也无法再跟上游合并。下面是按源码能落地的做法。

## 1. 结论

私有中继是 **另一个 cloud base URL**，说与官方 `cloud/` **同一套 WebSocket 线协议** 的一个子集。Host 和手机已经会把这个 URL 写进配对二维码，并用来拨号。密码学、channel 帧、审批、ACP 都不改。

要新写的是 VPS 上的服务。Host 只补一件今天做不到的事：把用户选择的 cloud URL **存进本机**，而不是只靠进程环境变量。手机不负责填写中继地址。

每台电脑同一时刻只有一个 cloud。官方 `https://api.codync.dev`（开发构建是 `https://dev-api.codync.dev`）和香港 VPS 是这台电脑的两个取值，来回切换，不能同时挂两条中继。另一个人的电脑，或这个人的第二台电脑，是另一份身份，各自一间 room，互不影响。第二台要新生成 `identity.json`，不要复制第一台的。局域网和 Tailscale 仍是直连候选，和 cloud 是竞速 / 回退关系，不是第四套协议。

个人自用的第一期只做 **本机扫码配对** 那条路。账号登录、Clerk、授权申请、Cloudflare TURN、公开 routine webhook 留在官方 cloud 上；指到私有中继之后，这些能力不可用，直到以后单独做。推播继续走现有的 APNs Worker，不搬到 VPS。

## 2. 原稿和源码的对照

| 原稿 | 源码事实 | 设计采取 |
| --- | --- | --- |
| 官方中继是 Cloudflare Worker 的 SSE 房间 | 聊天中继在 `cloud/`：WebSocket + 每台电脑一个 Durable Object（`cloud/src/relay.ts`）。SSE 只存在于本机 loopback 的 `GET /events` | 私有服务实现 §7 的 WebSocket，不发明 SSE 房间 |
| 新目录替代 `relay/` | `relay/` 是 APNs 推播 Worker。`cloud` 指账号和聊天中继的 base URL，`relay` 指推播（协议 §1） | 新目录叫 `private-relay/`。不改 `relay/`，不改 `cloud/` |
| Host 在 `crates/codync-host`，新增 `RelayMode` | Host 在 `host/`。出站连接是 `host/src/remote/relay.rs`，cloud URL 在 `host/src/remote/cloud.rs` | 不新增模式枚举。沿用现有 URL |
| 客户端设置页手填中继，扫码只是填充 | 权威在 host。二维码字段 `cloud=` 和每次 `hello` 的 `cloud` 都来自 `cloud::url`。手机把它 pin 进 `Computer.cloud` | 只在电脑上配置 URL。手机扫码或下一次成功的 `hello` 学会新地址 |
| 四种模式同时自由切换 | 手机每台电脑有 `ConnectionRoute`：先直连再 cloud、先 cloud 再直连、只用直连。文案写死 “Cloudflare”，实际拨的是 `Computer.cloud`。枚举的持久化名字不能改 | 保留这三档。cloud 指向哪台服务器由 host 决定 |
| 中继只做房间和二进制转发 | Durable Object 还要验 `Codync-Sig`、验 host 签名的 ACL、发 presence、持有离线 mailbox、限制配对 socket。它不解密 channel 帧。`hello` / `welcome` / `reject` 本身是签名明文 | 第一期实现这些，否则现有 host / 手机不会完成握手 |
| Hono + Redis 替代 Durable Object | DO 的状态是 SQLite（ACL、mailbox、nonce、blocked）。WebSocket 不能放进 Redis | 单进程 + SQLite。连接留在内存 |
| Mac 客户端和 iPhone 走同一条中继数据流 | 桌面 App 连本机 host 走 loopback（或 SSH `-L`）。走 E2E channel 的是 iPhone 和 Android。桌面 `hostRoute` 今天只有 `loopback` 或空 | 桌面负责配置和二维码。手机负责远程通道 |
| 改动 `shared` 以外的中继客户端分支 | `host/src/remote/crypto.rs`、iOS `RelayCrypto`、Android `core` 的帧加密已经与 cloud 无关 | 这些文件不动 |

## 3. 现况（实现时不要重做）

### 3.1 两条 Cloudflare 服务

```text
手机 / host
  ├─ cloud/     https://api.codync.dev 或 https://dev-api.codync.dev
  │             POST /v1/host/* 、 WebSocket /v1/relay/* 、 账号 API 、 routine webhook
  └─ relay/     iOS 编译进 SharedStore.relayURL
                Android 编译进 BuildConfig.PUSH_RELAY_URL（从同一条 Swift 常量读出）
                只登记 APNs ticket、转发密封过的通知
```

Host 出站中继：`host/src/background.rs` 启动 `remote::relay::run`。每条连接的顺序固定（`relay.rs` 文件头）：

1. `POST /v1/host/register`（这次监督循环里每个 base URL 一次；失败按退避重试，最长约 30 分钟。升级得到 4xx 时会重新注册。进程重启后重新来）
2. `GET /v1/relay/host?v=1`，带 `Codync-Sig`，升级 WebSocket
3. 最多等 10 秒拉 `GET /v1/host/state`。超时只打警告，照样发 ACL。账号来源的设备要等这次拉取成功
4. 发送签名 ACL，等到 `acl.ok`，再发 `ready`，然后发 `hooks`
5. 之后按 `link` 把设备复用到 `channel::run`；mailbox 一次一则，按序投递

`connected: true` 在插座一连上就置位，早于 ACL。日志里的 `relay ready` 才表示 `acl.ok` 已经到了、host 已经发出 `ready`。

Channel 外层：中继看得到包在 `data.m` 里的对象。`hello`、`welcome`、`reject` 是签名过的明文 JSON（协议 §6.1），里面有临时公钥、nonce、签名、配对标志、拒绝原因。只有 `f` 的 `d` 是 ChaCha20-Poly1305。Inner RPC 在 `f` 里面，中继看不到。

### 3.2 手机怎么选路

`apps/ios/Kit/Sources/CodyncKit/Client/ChannelTransport+Connection.swift` 的 `attempt()`，Android 同序在 `apps/android/core/src/main/kotlin/com/codync/android/core/HostConnector.kt`：

1. `cloudflareFirst` 且已有 cloud：先拨 `{cloud}/v1/relay/device/{computerId}?v=1`。拨号失败才改试下面的直连。拨号成功就停在中继上，不再平行直连。iOS 把拨号错误吞掉再落下；Android 只在 `IOException` 时落下，`SecurityException` 直接抛出。
2. 否则并行拨 `urls` 里每个地址的 `/channel?v=1`，第一个握完手的胜出。iOS 的预算默认 **1.5 秒**（三档一样）。Android 在 `directOnly` 或没有 cloud 时是 **15 秒**，否则 1.5 秒。
3. `automatic` 且直连都失败：再拨 cloud。
4. `directOnly`：从不拨 cloud。`cloudflareFirst` 拨号失败之后也不会再拨第二次 cloud。
5. App 回前台或网络路径变化：整段重跑，所以 Wi‑Fi 和蜂窝切换会自己重连。

`urls` 的生成在 `host/src/service/mod.rs` `addresses`：`CODYNC_EXTERNAL_URL`（逗号分隔，保持填写顺序）→ Tailscale → 局域网。Tailscale 含 `tailscale status --json` 的 DNS 名和 `100.64.0.0/10`，插完之后整组排序去重，DNS 名不保证排在地址前面。局域网是其余私网 IPv4，同样排序去重。

直连和中继用同一套 channel。中继多出来的只是外层 `presence`、`open` / `data` / `close` 和 mailbox。

`ConnectionRoute` 定义在 `apps/ios/Kit/Sources/CodyncKit/Models/Computer.swift`（原始值 `automatic`、`cloudflareFirst`、`directOnly`）和 `apps/android/core/src/main/kotlin/com/codync/android/core/Computer.kt`（枚举名 `Automatic`、`CloudflareFirst`、`DirectOnly`）。设置入口在 iOS `SettingsView.swift` 和 Android `SettingsView.kt`。桌面没有这条拨号；本机永远是 loopback。桌面 `ComputersView.tsx` 只有 cloud 开关（loopback `setCloud`）。

持久化名字不要改。iOS 按这些字符串解码已保存的电脑；Android 按枚举名解码。改名之后，手机上原来的路线会解不开。

### 3.3 cloud URL 今天从哪来

`host/src/remote/cloud.rs` 的 `url()`：

```text
CODYNC_CLOUD=off
  > kv cloud_enabled == false
  > 测试：kv test_cloud_url（仅 cfg!(test)，在环境变量之前返回）
  > 环境变量 CODYNC_CLOUD_URL（须为 https 根路径，或 http://127.0.0.1|localhost|[::1]）
  > 编译期默认：main = https://api.codync.dev，dev = https://dev-api.codync.dev
```

注释写明：**选哪一个 cloud 不落库，落库的只有开和关。** `setCloud` 只写 `cloud_enabled`，然后 `hub.cloud.config` 加一，relay 监督循环拆掉旧连接。

这个环境变量进不了已经装好的后台 host。`codync-host install` 写的 launchd / systemd 单元只放 `PATH` 和 `CODYNC_HOME`（`host/src/service/unix.rs`）。Windows 服务由 `codync-hostw` 拉起，只另设 `CODYNC_SERVICE`（`host/src/service/windows.rs`），同样没有用户的 cloud URL。桌面进程里的 `CODYNC_CLOUD_URL` 只影响桌面自己的账号 API（`apps/desktop/src/main/account.ts`）。装好的 host 是服务，不继承桌面进程的环境。`npm run dev` 若在同一个 shell 里前台跑 `codync-host serve`，这个变量 host 和桌面都看得到；那是阶段 A，不是装好的服务。

所以「在 shell 里 export 一下」对已经装成后台服务的 Codync 无效。第一期 host 改动就是把 URL 存进 kv，让 `codync-host cloud` 和桌面设置能写它。

二维码已经带 cloud（`host/src/service/mod.rs` `pairing_url`）：

```text
codync://pair?v=3&name=…&id=…&sk=…&bk=…&code=…&urls=…&cloud=<base>
```

开发构建的 scheme 是 `codync-dev://`。`urls` 和 `cloud` 至少要有一个。`cloud` 会做百分号编码。

手机对 cloud 的接受范围比 host 窄：

| 端 | 规则 |
| --- | --- |
| iOS `Pairing.isCloudURL` | https。Debug 另外允许 `http://127.0.0.1` 和 `http://localhost`，不允许 `[::1]` |
| Android `isCloudUrl` | 只接受 https，任何构建都一样。无 user、password、query、fragment |

二维码里的 `cloud` 通不过这张表时，整张码作废（iOS 抛 invalid，Android `Computer.validate` 失败）。不会只丢掉这个字段。iOS 模拟器的 `127.0.0.1` 是 Mac；Android 模拟器的 `127.0.0.1` 是模拟器自己，而且 Android 根本不接受 http cloud。

连上之后的 `hello`：iOS 把不合法的 `cloud` 收成空，已经保存的地址也会被清掉。Android `HostClient.hello` 把 `cloud` 原样写上再 `validate()`，不合法会让这次 hello 失败。所以写给手机的地址必须是公网 https。`http://[::1]` 只够 host 自己连本机服务，不能进二维码。

`hello` 响应带 `cloud`（`host/src/api/host.rs`，没有 cloud 时是 JSON null）。连上之后手机会把新的 `urls` 和 `cloud` 合并进已保存的 `Computer`（协议 §7.5 第 5 步）。

### 3.4 信任边界（私有中继也不放宽）

- 新设备只有两条路：本机扫码 `pair`，或电脑上的人批准账号申请。Cloud 的 state **只能续租或删除**已有的账号设备，不能加设备（`cloud.rs` 的 `apply_grants` 有测试钉住这一点）。
- 成功的 state 拉取里，账号设备的 grant 不在清单上时，**当场** `revoke_device`。15 分钟租约只用于拉不到 state 的时候（连不上、或这次拉取失败）。空的 `grants` 不是「再等 15 分钟」。
- ACL 由 host 签名。中继用 host 公钥验签，不重新解释 JSON，也不把 JSON 再序列化一遍再验。
- 每次 channel 连接做一次临时 X25519，密钥用完即丢。中继不参与。
- 审批、shell、文件沙盒在 host。中继没有对应的方法。

私有中继换的是管道的运营方，不提高 §6 的密码学强度。管道看得到：谁在线、连接时间、帧大小、device key 公钥、ACL 里的设备列表和 grant id、`hello` / `welcome` / `reject` 的明文字段、mailbox 的 nonce 和密文 blob。看不到 `f` 里的对话明文，也看不到 mailbox 解开后的 `send`。

### 3.5 不在这条管道里的能力

| 能力 | 走哪 | 指到私有 cloud 之后 |
| --- | --- | --- |
| 推播 | `relay/`，URL 编译进 App | 仍然走官方 Worker。通知正文仍由 host 密封。VPS 看不到，也关不掉这条元数据，除非以后改 App 里的常量并自备 APNs 密钥 |
| 远程画面的 TURN | 手机走中继时 `screenPrepare` 带 `relay: true`，host `POST /v1/host/screen-ice`。直连时 `relay: false`，不向 cloud 要凭据 | 第一期这次 POST 会失败。手机把 host 返回的错误原文显示出来（官方未登录时是 “Sign in on the computer to use remote screen relay.”；私有中继的 404 会变成 “the Codync cloud refused (404 …)”）。同一局域网的直连画面不经过 TURN，继续可用 |
| 账号、Ask for access、SAS | `cloud/` 的 Clerk + D1 | 第一期不实现。`/v1/host/state` 固定返回空 grants。见 §4：这会立刻删掉本机的账号设备 |
| Routine 的公网 webhook | `{cloud}/v1/hooks/{computerId}/{routineId}`，URL 由 host 用当前 cloud base 拼出来 | 第一期该 URL 返回 404。聊天不受影响：host 发出 `hooks` 后不要求 `hooks.ok`（`relay.rs` `on_message` 对未知 `t` 直接忽略）。`badHooks` 会被记成 error 日志，但不会拆连接 |

## 4. 目标行为

电脑上的 cloud 取下面三者之一：

| 取值 | 手机在自动路线下的外网路径 |
| --- | --- |
| 关 | 只有 `urls`（局域网、Tailscale、`CODYNC_EXTERNAL_URL`） |
| 默认 | 官方 `api.codync.dev` 或 `dev-api.codync.dev`，与现在相同 |
| 自定义 | `https://relay.example.com` 这种根 URL，香港 VPS |

手机上的三档路线保持原样。文案从写死的 “Cloudflare” 改成随 `Computer.cloud` 的主机名变化，见 §9。

已经配对过的手机要学会新 URL，需要下面任一条件：

1. 重新扫当时的二维码（二维码里的 `cloud=` 已是新地址），或
2. 还有一条能握上手的直连（家里 Wi‑Fi 或 Tailscale）。这次 `hello` 会把新 `cloud` 写进手机。

只改 host、又让手机停在 `directOnly` 或停在旧 cloud 上，手机不会自己发现 VPS。切换顺序：

1. 若这台电脑还在官方账号里，**先**在还能连上官方 cloud 时 Remove from account（`POST /v1/host/unclaim`）。切到私有中继之后，unclaim 会打到 VPS 并 404，官方 D1 里的电脑记录就留在那里，直到把 URL 指回官方再解除。
2. 在电脑上写入私有 URL。`codync-host cloud` 的 `connected: true` 只表示插座连上。还要在 host 日志里看到 `relay ready`。
3. 手机扫新二维码，或在同一网络上打开一次 App。
4. 切回官方：清空自定义 URL。手机同样要再扫一次或再直连一次。

空的 state 在拉取成功时立刻删掉 `source=account` 的设备行，不是等 15 分钟租约。本机扫码得到的设备（`source=local`）不受 state 影响，切 URL 不用重新批准，但要让手机拿到新的 `cloud`。

## 5. 数据流

```text
iPhone / Android
    先试 urls → ws(s)://<直连>/channel?v=1
    否则     → wss://<cloud>/v1/relay/device/<computerId>?v=1
              Codync-Sig（device key）
                      │
                      ▼
              private-relay（单进程）
              验签、按 ACL 准入、原样转发外层 JSON
              SQLite：电脑登记、ACL、nonce、mailbox
                      │
                      ▼
              wss://<cloud>/v1/relay/host?v=1
              Codync-Sig（host sign key）
                      │
                      ▼
              codync-host
              channel 解密、验签、防重放、审批、ACP
```

桌面 App 不在这条链上。它用 loopback 调 `setCloud` / 新的 `setCloudUrl`，并显示二维码。

中继转发 `data.m` 时看得到 `hello` / `welcome` / `reject` 的明文。它不解析这些签名，只检查 `t` 和大小。验配对 code 的是 host。

配对（协议 §4.1，中继侧）：

1. 电脑展示二维码。Host 生成一次性 code（16 字节），立刻发布带 `offers` 的 ACL。
2. 手机用二维码里的 `sk` 连上。中继路径的查询参数是 `pair=<offerId>`，`offerId = b64url(SHA-256("codync/offer/v1" ‖ code)[0..16])`（`crypto::offer_id`）。`pair` 必须是 16 字节的 b64url，否则 `400`，并且不要为此创建 room。
3. 中继确认 offer 未过期且在最新 ACL 里，分配 `link`（12 字节随机，b64url），向 host 送 `open`。
4. 双方在该 link 上做 §6 握手。Host 验 code，写入 `source=local` 的设备，关闭这条 channel，close code `4100`、reason `paired`。
5. 手机立刻不带 `pair` 重连。中继改按 ACL 的 `devices` 准入。

## 6. 明确不做

- 不改 `host/src/remote/crypto.rs`、iOS `RelayCrypto`、Android 帧加密、向量文件 `docs/reference/fixtures/remote-relay-vectors.json`。
- 不改 inner RPC、SSE 事件形状、审批方法、ACP / MCP。
- 不在 `cloud/` 或 `relay/` 里加私人分支。官方部署保持原样。
- 不做对外的多租户，不做账号体系，不做「一台电脑同时挂四条中继」。同一个人的多台电脑不是多租户：每台自己注册，中继按 `computerId` 分房间。
- 不把 WebSocket 放进 Redis，也不为这几台电脑做多进程。多进程必须按 `computerId` 粘滞，第一期不需要。
- 不把中继地址做成手机上的权威设置。手机上多填一个 URL，host 不会去连它。
- 不改 `ConnectionRoute` 的持久化名字。
- 第一期不实现 Clerk、access request、`/internal/block`、screen-ice、webhook 队列。
- 不把 APNs 密钥放到 VPS。

## 7. `private-relay/` 服务

新目录，Node.js 22 或更新（`node:sqlite` 从这条线才有；Ubuntu 自带的 Node 太旧。不想升 Node 就用 `better-sqlite3`）。与 `cloud/`、`relay/` 并列。依赖尽量少：HTTP 用 Hono 可以，WebSocket 用 `ws`，Ed25519 用 `@noble/ed25519`（与 `cloud/test/ref.ts` 同一套验签）。进程守护用 systemd。PM2 能用，但单服务用 systemd 更贴近 Ubuntu 上的 host 自己的做法。

参考实现是 `cloud/src/relay.ts` 的 `ComputerRelay`，加上 `cloud/src/routes/relay.ts` 和 `cloud/src/auth.ts` 里 WebSocket 升级与 `Codync-Sig` 那一段。Durable Object 的休眠、`WebSocketPair`、自动 ping 响应要换成普通进程里的等价行为，线格式不变。Node 没有 `WebSocketPair`。两条 WebSocket 路由在 HTTP server 的 `upgrade` 事件里交给 `ws`，其余请求交给 Hono。不要把升级塞进 Hono 的 fetch 适配。

### 7.1 进程和存储

一个进程。内存里 `Map<computerId, Room>`，这个人的每台电脑一间，ACL 和 mailbox 不共用。`Room` 持有：

- 至多一条 host WebSocket，以及它的 `kid`、`ready`
- 至多 16 条 device WebSocket（配对 socket 算在这 16 条里），每条有 `dk`、`link`、`pair`、`offer?`
- 每条 device 的 10 秒速率窗（消息数、字节数）。进程重启后窗清空，与 DO 休眠后清空相同，协议允许

SQLite 单文件，例如 `/var/lib/codync-private-relay/relay.sqlite`，权限 `0600`：

| 表 | 用途 | 对照 |
| --- | --- | --- |
| `computers(id, sign_pub, box_pub, name, platform, device, version, status, updated_at)` | `POST /v1/host/register` | D1 `computers` 的子集。`status` 只有 `active`。第一期注册成功就写 `active` |
| `acl(computer_id, ver, body, updated_at)` | 最新一份验过签的 ACL JSON 字节 | DO `meta` |
| `nonces(kid, nonce, exp)` | `Codync-Sig` 10 分钟重放。HTTP 和 WebSocket 共用 | 官方 HTTP 在 D1 `sig_nonces`，中继在 DO `nonces`。这里合成一张表 |
| `mailbox(seq, computer_id, dk, nonce, blob, size, exp, state)` | 离线信箱，`UNIQUE(computer_id, dk, nonce)` | DO `mailbox`。一台电脑一个 DO，所以官方的唯一键是 `(dk, nonce)` |
| `blocked` | 第一期可以不建。没有账号撤权就没有写入方 | DO `blocked` |

不存：对话、inner JSON、审批内容、文件、密钥材料。日志不写 WebSocket 文本正文，只写 `computerId`、角色、关闭码、字节数、错误码。

Nginx 放在前面做 TLS。Node 只听 `127.0.0.1`。

### 7.2 HTTP

Host 读错误时只用 `error.code`，`error.message` 可空，会进 `codync-host cloud` 的 `last error`。`requestId` 可以不返回。时间是 Unix 毫秒。

| 方法 | 行为 |
| --- | --- |
| `GET /v1/health` | `{"ok":true,"version":"<private-relay 版本>"}`。这是私有服务自己的版本，不是 Codync 的 `MARKETING_VERSION`。Host 不调用它 |
| `POST /v1/host/register` | 要 `Codync-Sig`，kid 为 32 字节 host 签名公钥。`computerId = b64url(SHA-256(kid)[0..16])`，必须是 22 位 `[A-Za-z0-9_-]`，由 kid 推导，不接受 body 里另给的 id。Body 收 `boxKey`（32 字节）、`name`、`platform`（只允许 `macos`、`linux`、`windows`）、`device`、`version`。返回 `{"computerId","owned":false}`。Host 只核对 `computerId` 与自己的是否相同，不看 `owned`；不对就拒绝这次注册并重试 |
| `GET /v1/host/state` | 要 `Codync-Sig`。固定返回 `{"owner":null,"grants":[],"requests":[]}`。拉取成功后，host 会立刻删掉本机所有 `source=account` 的设备 |
| `GET /v1/relay/host?v=1` | WebSocket 升级。先验签。未注册 → `404 unknownComputer`。`status` 不是 `active` → `403 forbidden`。签名错 → `401 badSignature` |
| `GET /v1/relay/device/{computerId}?v=1` | 可选 `pair`。`v` 不是 `1` → `426 upgradeRequired`。`computerId` 或 `pair` 格式不对 → `400`，不创建 room。未注册或不是 `active` → `404 unknownComputer`（设备和 host 这条不一样：设备侧不区分 forbidden）。签名错 → `401` |
| 其他 `/v1/*` | `404`，body 仍是 `{"error":{"code":"…"}}`。包括账号路由、`/v1/host/screen-ice`、`/v1/host/unclaim`、`/v1/hooks/…` |

`POST /v1/host/grants/{id}/revoke` 也走这张 404。Host 把 **404** 当成「云上已经没有这条 grant」，并清掉本地重试队列。不要回 500，否则撤权会一直重试。

`Codync-Sig`（协议 §5），验签输入是 UTF-8、换行分隔、无结尾换行：

```text
codync-sig-v1
<METHOD 大写>
<authority：小写主机，默认端口不写，非默认端口要写>
<path + query，与请求行逐字相同>
<ts>
<nonce>
<b64url(SHA-256(body))>    GET 和 WebSocket 用空 body 的 hash
```

Header 形如 `v=1,kid=<32 字节>,ts=<毫秒>,nonce=<16 字节>,sig=<64 字节>`，都是 b64url。`|now - ts| ≤ 300_000`。`(kid, nonce)` 10 分钟内不可重复（`NONCE_TTL_MS = 600_000`）。

Authority **不是**照抄 `auth.ts` 的 `new URL(request.url).host`。Worker 上那个 URL 就是公网地址。Nginx 后面 Node 看到的 URL 往往是 `127.0.0.1:8788`，拿它验签会全部 `401 badSignature`。用客户端签进去的那个名字：Nginx 转来的 `Host`，默认 443 不带端口。`host/src/remote/cloud.rs` 的 `authority()` 对 `https://x.dev:443` 得到 `x.dev`，对 `http://127.0.0.1:8787` 得到 `127.0.0.1:8787`。所以示例用 `$host`，不要用 `$http_host`。也不要改写 path 或 query。向量是 `docs/reference/fixtures/remote-relay-vectors.json` 的 `requestSig`。实现直接对这组向量做测试。

个人使用可以不做「每 IP 每分钟 10 次新建电脑」。要做就放在 Nginx `limit_req`，不要做进业务表。

### 7.3 WebSocket 外层

客户端看到的 URL 就是上面两行。官方 Worker 会把升级转成带 `X-Codync-Internal` 的内部请求；私有服务没有这层 DO，验签之后直接 `accept`。不要把内部头暴露成另一种客户端协议。

心跳：host 和手机每 30 秒发送 **完全相同** 的字符串 `{"t":"ping"}`（`relay.ts` 的 `PING`）。服务回复 **完全相同** 的 `{"t":"pong"}`。服务自己不必再发 ping。官方靠 DO 的自动响应，不唤醒业务逻辑；这里在消息回调里回复即可。任何 socket 超过 90 秒没有入站消息（ping 算）就关掉，关闭码 `1011`。DO 会先退役、不立刻 `close()`，那是为了休眠计费；普通进程没有这个问题，直接关。服务每 60 秒扫一次过期 mailbox 和 nonce。

Host 连上时如果已有一条 host socket，旧的以 `4009 replaced` 关闭，所有 device 先收到 `presence online:false`。新 host 在 `ready` 之后才是在线。在线的唯一定义：存在 host socket 且 `ready == true`。

单条 WebSocket 消息 ≤ 1 MiB。Device → 中继：每条 socket 每 10 秒 ≤ 200 条且 ≤ 8 MiB，超出 `4008`。Host → device 不限速。

### 7.4 Host 消息

Host → 中继：

| `t` | 第一期 |
| --- | --- |
| `acl` | 必须。`d` 是 ACL JSON 字节的 b64url，`sig` 是 host kid 对 **这些字节** 的 Ed25519。验签对象是解码后的原始字节，不是再 `JSON.parse` 再序列化的结果。验 `computerId`、`v == 1`、`ver` 为不小于 1 的安全整数。还没有存过 ACL 时，`ver >= 1` 即可。`ver` ≤ 已存版本 → `{"t":"error","code":"staleVersion","ver":<已存>}`。签或形状错 → `{"t":"error","code":"badAcl","ver":<已存或 0>}`。两种错误都要带 `ver`：host 用它把本地 `acl_ver` 抬过中继，然后重发一次；第二次再失败就拆掉这条中继连接。验签失败时保留旧 ACL。成功则保存，回 `{"t":"acl.ok","ver":N}`，并关掉 dk 已不在 `devices` 里的普通 device socket（`4003`），关掉 offer 已不在的配对 socket（`4410`） |
| `ready` | 把 host 标为 ready，向每个 device 送 `presence online:true`，向 host 为每条 device 送 `open`，开始投递 mailbox |
| `data` | `{link, m}`。host 必须已 ready。把 `m` 用 `JSON.stringify` 写到该 link 的 device socket，设备看到的是 channel 对象本身（`welcome` / `reject` / `f`），不是包一层 `data`。不要拆 `d` |
| `close` | `{link, code, reason}`，用这个 code 关闭该 device socket。`4100` 由 host 在配对完成后送来，照转 |
| `mbox.ack` | 见 §7.6 |
| `hooks` / `hook.ack` | 接受并忽略 `hooks`（可回 `{"t":"hooks.ok","n":0}`）。不要回 `badHooks`。没有投递就不产生 `hook.item` |
| `ping` | 回 pong |

中继 → host：`open {link, dk, pair}`、`data {link, m}`、`close {link}`、`mbox.item`、`acl.ok`、`error`。第一期不发 `cloud.changed` 和 `hook.item`。Host 另外每 5 分钟拉一次 state，不靠 `cloud.changed`。

ACL 形状（`grant` 对个人配对是 JSON null，来自 `grant_id` 为空）：

```json
{"v":1,"computerId":"…","ver":12,
 "devices":[{"dk":"…","grant":null}],
 "offers":[{"id":"<offerId>","exp":1790000600000}]}
```

### 7.5 Device 消息

准入，在 `accept` 之前：

- 电脑未注册，或还没有任何 ACL → `404 unknownComputer`
- 带 `pair`：offer 在 ACL 里且 `exp > now`，否则 `403`。同时最多 2 条配对 socket，超出 `429`。每条最多 120 秒、最多 16 条消息（第 17 条 `4008`）。不允许 `mbox.*`，回 `{"t":"mbox.err","nonce":…,"code":"pairing"}`
- 不带 `pair`：`dk` 在最新 ACL 的 `devices` 里，否则 `403`。第一期没有 `blocked` 表
- device socket 超过 16 条 → `429`

连上后立刻向该 device 送一次 presence，形状与 `relay.ts` 的 `presence()` 相同：

```json
{"t":"presence","online":false,"lastSeenAt":null}
```

`lastSeenAt` 是 host 上次变为 ready 或离开时的毫秒，没有过就是 null。缺这个字段时，手机的离线时间是空的。若 host 已 ready，立刻向 host 送 `open`。

Device → 中继只转发 `hello` 和 `f`：原样包成 `{"t":"data","link","m"}` 给 host。Host 不在线则丢掉，并再送一次 presence。不要尝试解密。`ping` 回 pong。`mbox.put` / `mbox.cancel` / `mbox.list` 按 §7.6 处理，不要转给 host。其他 `t` 以 `4002 protocolError` 关闭。不要把「握手前的任何对象」都转发。

`t` 不与 channel 重叠。Channel 只有 `hello`、`welcome`、`reject`、`f`。

### 7.6 Mailbox

手机在电脑离线时排队 `send`，也会 `mbox.list` 和 `mbox.cancel`（iOS `ChannelTransport+Mailbox.swift`，Android `EncryptedChannel.kt`）。规则用协议 §6.4 和 §7.7，中继只存 blob：

- 仅当没有 ready 的 host 时接受 `mbox.put`。Host 在线 → `{"t":"mbox.err","nonce","code":"hostOnline"}`
- `(computerId, dk, nonce)` 已存在 → `{"t":"mbox.ok","nonce","state":<现有>}`，不插第二行
- blob 是 `epk(32) ‖ sig(64) ‖ 密文`。短于 112 字节 → `invalid`。长于 64 KiB → `tooLarge`。nonce 空或长于 128 → `invalid`。`exp` 缺省 now + 24 小时，给出的值必须是 now 之后、且不超过 now + 24 小时，否则 `invalid`
- 每 dk ≤ 50；每电脑 ≤ 500；合计 ≤ 8 MiB。超出 → `full`
- 成功 → `{"t":"mbox.ok","nonce","state":"queued"}`
- `mbox.cancel`：状态还是 `queued` → 删除并回 `{"t":"mbox.cancelled","nonce"}`。已经是 `delivering`，或不存在 → `{"t":"mbox.gone","nonce","state":"delivering"|"unknown"}`
- `mbox.list` → `{"t":"mbox.items","items":[{"nonce","exp","state":"queued"|"delivering"}]}`，只列该 dk 的，按 `seq`
- host `ready` 后按 `seq` **一次送一则** `mbox.item`，状态改 `delivering`。等 `mbox.ack` 再送下一则。Host 的实现假定未确认的只有一则
- `mbox.ack` 且 `ok: true` → 删除并通知 `mbox.delivered`。`ok: false` → 删除并通知 `mbox.failed`，`code` 只接受 `unauthorized`、`unknownBot`、`invalid`，其他写成 `invalid`
- host 断开时，`delivering` 改回 `queued`
- 到期删除并尽量通知还连着的 device：`mbox.expired`

Host 会自己解密、验签、用 `clientNonce` 去重。中继不要打开 blob。

### 7.7 关闭码

与协议 §7.6 相同，host 和手机会根据这些码决定重试还是停：

`1000`、`1011`、`4001`、`4002`、`4003`、`4004`、`4008`、`4009`、`4011`、`4013`、`4100`、`4400`、`4410`。

`4100` 由 host 在配对完成后通过 `close` 发来，中继照转，不要自己发明配对状态机。手机收到 `4100` 会立刻不带 `pair` 重连。`4009` 时 host 等 5 分钟再试。

### 7.8 建议文件

```text
private-relay/
├── package.json
├── src/index.ts       # 监听 127.0.0.1；upgrade 交给 ws，其余交给 Hono
├── src/http.ts        # health、register、state
├── src/sig.ts         # Codync-Sig 与 computerId；authority 用公网 Host
├── src/room.ts        # 一台电脑的 socket、ACL、mailbox（从 ComputerRelay 删掉 hook 和 block）
├── src/store.ts       # SQLite
└── test/              # 向量、ACL 版本、mailbox 幂等、配对 offer 过期、mbox.cancel / list
```

`room.ts` 控制在协议状态机一个文件能看完的篇幅。超过大约 500 行就按「准入 / 转发 / mailbox」拆开。

## 8. Host 改动

只动 cloud URL 的来源和文案所需的状态。`relay.rs` 的状态机、`crypto.rs`、channel、二维码字段都不改。`relay.rs` 已经把 base URL 转成 `wss://<host>/v1/relay/host?v=1`（`relay_url`；`http://` 转成 `ws://`）。

### 8.1 落库

`cloud::url()` 改为：

```text
CODYNC_CLOUD=off
  > kv cloud_enabled == false
  > cfg!(test) 时的 kv test_cloud_url     # 留在环境变量之前，单测不能打到真云
  > CODYNC_CLOUD_URL                      # 仍给本地 wrangler / 前台 serve，优先级高于用户设置
  > kv cloud_url                          # 新：用户的私有 base，无尾部斜杠
  > 编译期默认
```

校验复用 `valid_url`：调用前先补上尾部斜杠，要求路径 `/`、无 query；https，或明文 http 仅 `127.0.0.1`、`localhost`、`[::1]`。非法值拒绝，不写入。存进去的值没有尾部斜杠。

新的 loopback 方法 `setCloudUrl`。分发加在 `host/src/api/remote.rs` 里 `setCloud` 旁边。权限加进 `host/src/api/devices.rs` 的 `LOCAL_ONLY`，与 `setCloud` 一样只允许 `Caller::Local`。漏了这张表，手机就能改这台电脑的中继地址。

```json
{"url": "https://relay.example.com"}
```

`url` 为空字符串或 `null` 表示清除，回到编译期默认。成功后走和 `set_cloud` 一样的 `hub.cloud.config` 通知，`relay::run` 会拆掉旧连接并用新 URL 重新 `register`。

统计：`host/src/analytics/observe.rs` 不认识的方法没有事件。要记的话加进 `SETTINGS`，只记「设置了自定义地址」或「清空了」，不要把 URL 或主机名放进属性。

`cloudStatus` 已有 `url` 字段，桌面和 CLI 不用新事件。

CLI：`codync-host cloud` 增加 `--url <https-origin>` 和 `--clear-url`。打印保持现有七行：`enabled`、`url`、`registered`、`connected`、`account`、`approval`、`last error`。

终端界面：`host/src/tui/` 今天没有 cloud 设置页，也没有 “Cloudflare” 文案。对应的终端入口就是这条 CLI，不另做一块 TUI 画面。

### 8.2 日志

`relay.rs` 已有 `computer_id`，连上时的句子是 `connected to the relay`，ACL 成功后是 `relay ready`。连上和失败时再加一个结构化字段 `cloud_host`（只要主机名，不要路径，也不要把 URL 写进分析事件）。不要给官方 / 私有做两套连接代码。

### 8.3 兼容

`hello.cloud` 仍是字符串或 null。`setCloudUrl` 是新的 loopback 方法，手机调用不到。不改 channel 的 `v`，不提高 `minApp`。

这是 `host/` 的源码改动。合进 `main` 之前按仓库规则做一次 2.x 补丁版本号（`apps/project.yml`、`host/Cargo.toml`、`apps/desktop/package.json` 三处一起）。`private-relay/` 本身不算这三处，单独目录不触发 Codync 版本号；一旦改了 host 或 App，就要升。

## 9. 客户端改动

拨号代码不用加分支。iOS `relayRequest` 和 Android `HostConnector` 已经对任意 `Computer.cloud` 签名并连接。

要改的是写死 “Cloudflare” 的句子，让用户分得清官方云和自己的域名。判断用已保存的 `cloud` 主机名：

| `Computer.cloud` | 直连时 | 走中继时 |
| --- | --- | --- |
| 空 | Wi‑Fi or Tailscale | 不会走到中继 |
| 主机是 `api.codync.dev` 或 `dev-api.codync.dev` | 同现在 | 保持 “Cloudflare” |
| 其他 https 主机 | 同现在 | “Private relay” 加上主机名，例如 `relay.example.com` |

用户看得到的句子：

| 位置 | 现句 |
| --- | --- |
| iOS `apps/ios/Views/SettingsView.swift` | 路线菜单三档，以及状态行 “ · Cloudflare” |
| iOS `apps/ios/Views/ComputerStatus.swift` | 无障碍标签 “Through Cloudflare” |
| iOS `apps/ios/Views/PairingView.swift` | “from anywhere through Cloudflare” |
| iOS `apps/ios/Kit/Sources/CodyncUI/Thread/ThreadView.swift` | “Connected through Cloudflare”（手机走中继时聊天标题会显示） |
| Android `apps/android/app/src/main/kotlin/com/codync/android/SettingsView.kt` | `ConnectionRoute.title`，以及机器人数量那一行的 “Cloudflare” |
| Android `…/ConnectionPresentation.kt` | “Connected through Cloudflare” |
| Android `…/PairingView.kt` | 与 iOS 配对页同一句 |
| 桌面 `apps/desktop/src/renderer/views/settings/ComputersView.tsx` | `cloudLine` 的三句：关掉、正在连接、已连通 |

桌面 `ThreadView.tsx` 里也有 “Connected through Cloudflare”，但 `bot-sync.ts` 把本机 `hostRoute` 设成 `loopback`，SSH 电脑是空。这句今天不会出现在桌面聊天标题上。改的时候一起改掉，验收不要在桌面标题里找它。本机标题仍是 “Connected locally”。

路线菜单的三档语义不变，只把 “Cloudflare” 换成当前 cloud 的称呼。`directOnly` 仍是「只用 Wi‑Fi 或 Tailscale」。

这些英文原句是翻译键。桌面 `apps/desktop/src/renderer/lib/l10n.ts` 直接读 `apps/ios/Kit/Sources/CodyncKit/Resources/Languages/*.json`（zh-Hant、zh-Hans、ja、ko、de、es、fr）。改句子就要改键和各语言译文。带主机名的句子用 `{host}` 占位，不要把域名拼进键里。Android 的这几句是 Kotlin 里的英文字面量，不走那份 JSON，意思与 iOS 对齐即可。

桌面 `ComputersView` 在 “Reach from anywhere” 下加一个地址栏：保存调用 `setCloudUrl`，清空则清除。校验失败显示 host 返回的错误。不要在渲染进程里自己拼 WebSocket 地址。

iOS / Android 不提供地址输入框。它们只显示 host 通过 `hello` / 二维码给的那一个。

TUI 不显示这些英文句子，没有对应画面要改。终端用 §8.1 的 CLI。

桌面、iOS、Android 的这条文案属于用户看得到的界面，三边同一批改。iOS 文案进入发版说明时，再写 `apps/ios/WhatsNew.md` 的一节。

## 10. 部署

一台香港 VPS，Ubuntu 22.04 或 24.04，1 核 1 GB 够一个人名下的几台电脑共用。自有域名，A 记录指向这台机器。Node 用 22 或更新，不要用发行版自带的旧 Node，除非改用 `better-sqlite3`。

| 组件 | 要求 |
| --- | --- |
| Nginx | 公网 443，TLS 1.2+，**不要开 `http2`**。手机和 host 的 WebSocket 是 HTTP/1.1 的 `Upgrade`。`listen … http2` 会经 ALPN 选上 h2，升级失败，表现为连上就断。`proxy_read_timeout` 和 `proxy_send_timeout` 至少 3600 秒。只在 `Upgrade` 有值时把 `Connection` 设为 `upgrade`，普通的 `POST /v1/host/register` 不要带这个头。转给 Node 的 `Host` 用 `$host`（公网域名，不带 `:443`）。不要改写 path |
| 证书 | Let’s Encrypt，`certbot` 续期。续期失败是唯一需要看的日常项 |
| Node | 只听 `127.0.0.1:8788`（端口自定，不进公网） |
| SQLite | 文件在 `/var/lib/codync-private-relay/`，用户是专用系统用户 |
| 防火墙 | 公网只开 443。80 仅用于证书挑战，完成后可以关掉。SSH 只允许密钥，端口和来源 IP 按自己的习惯收紧 |
| Redis | 不安装。没有它要承担的状态 |

Nginx 不记 `access_log`。排障时临时打开，用完关掉。`error_log` 保留。Node 日志同样不包含消息正文。

示例（域名和端口按实际替换）：

```nginx
map $http_upgrade $connection_upgrade {
    default upgrade;
    ''      close;
}

server {
    listen 443 ssl;
    server_name relay.example.com;
    ssl_certificate     /etc/letsencrypt/live/relay.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/relay.example.com/privkey.pem;
    access_log off;

    location / {
        proxy_pass http://127.0.0.1:8788;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection $connection_upgrade;
        proxy_read_timeout 3600s;
        proxy_send_timeout 3600s;
    }
}
```

systemd 单元：`Restart=always`，`RestartSec=2`，`NoNewPrivileges=true`，`PrivateTmp=true`。不要用 root 跑 Node。

备份：SQLite 里没有对话。丢了文件的后果是手机要重新扫码（ACL 和 mailbox 没了，host 会重新发布 ACL）。电脑上的 `identity.json` 仍然是身份根，不要为了中继去轮换它。

## 11. 分阶段

阶段 A–C 已在这棵树里：`private-relay/`、host 的 `setCloudUrl`，以及 iOS / Android / 桌面的私人中继文案。下面的完成标准仍是验收清单。阶段 D 没有做。

### 阶段 A — 协议服务，不改 Codync 产品代码

用前台 `codync-host serve` 做开发验证（这时 `CODYNC_CLOUD_URL` 能进进程）。装成 launchd / systemd / Windows 服务之后要等阶段 B。

`CODYNC_CLOUD_URL=http://127.0.0.1:<port>` 只覆盖 host 自己和 **iOS Debug 模拟器**。iOS 模拟器的 `127.0.0.1` 就是这台 Mac。Android 不接受 http cloud，扫这张码会失败；Android 要到有 https 的 VPS 再测，或者阶段 A 只走 iOS。

完成标准：

- 向量测试：`requestSig`、ACL 验签（对 `d` 的原始字节，不是重新序列化的 JSON）、computerId 推导
- 本机：`CODYNC_CLOUD_URL=http://127.0.0.1:<port>`，host 日志出现 `relay ready`。`connected: true` 不够，它在 ACL 之前就有了
- iOS Debug 模拟器扫码配对、发一条消息、收到回复、批准一张权限卡
- 关掉 Node 再打开，host 自己重连；手机退到后台再打开，事件从当前 `rev` 补上
- 电脑进程停着时，手机 `mbox.put` 入队；电脑起来后只执行一次（`clientNonce` 去重）。同一 nonce 再 put 一次，返回已有状态。`mbox.list` 和 `mbox.cancel` 有响应
- 官方 cloud 不设置该变量时行为与现在相同

### 阶段 B — host 记住 URL

`setCloudUrl`、`codync-host cloud --url`、桌面地址栏。安装出来的后台 host 不用改 plist 就能指向 VPS。

完成标准：桌面里填入 `https://relay.example.com`，`codync-host cloud` 的 `url` 变为该域名，日志出现 `relay ready`。清空后回到 `api.codync.dev` 或 `dev-api.codync.dev`。已配对手机在同一 Wi‑Fi 上打开一次后，断掉 Wi‑Fi，仍能经 VPS 收发。

### 阶段 C — 三端文案

§9 的称呼，连同语言 JSON。完成标准：官方 cloud 仍显示 Cloudflare；私有域名显示该主机名；`directOnly` 仍显示 Wi‑Fi / Tailscale。iOS、Android、桌面各看一眼。桌面聊天标题仍是本地连接，不作为这条的验收点。

### 阶段 D — 以后才做，单独开设计

按需要选，不要和 A–C 捆在一起：

- Routine webhook（协议 §7.8）。明文到达 VPS，和官方 cloud 一样，不是端到端。界面要照实写。
- 远程画面：在同一台 VPS 上跑 coturn，`POST /v1/host/screen-ice` 的 JSON 要能反序列化成 `IceConfig`（`iceServers`：`urls`，以及可选的 `username`、`credential`；`expiresAt` 必须晚于现在 60 秒以上，`iceServers` 不能是空的，否则 host 拒绝）。凭据格式以 `host/src/screen/viewer.rs` 为准。不要去申请 Cloudflare Realtime。
- 推播自建：改 iOS `SharedStore.relayURL` 和 Android `PUSH_RELAY_URL`（`apps/android/app/build.gradle.kts` 从那条 Swift 常量读默认值），并在 VPS 上持有 APNs 密钥。这和聊天中继是另一个信任决定。

## 12. 测试

| 层 | 怎么测 |
| --- | --- |
| `private-relay` 单测 | 读 `remote-relay-vectors.json`。覆盖：签名 authority 含端口、默认 443 不带端口、过期 ts、重放 nonce、ACL `staleVersion` 与 `badAcl` 都带 `ver`、配对 offer 过期、`pair` 不是 16 字节时 400 且不建 room、mailbox 同一 nonce 两次 put、host 在线时拒绝 put、短于 112 字节的 blob、`mbox.cancel` / `mbox.list`、一次只投递一则、16 条 device 上限、ping 字符串必须全等 |
| Host | 现有 `host/src/remote/relay.rs` 测试继续打假 cloud。阶段 B 给 `url()` 加：测试里仍然只认 `test_cloud_url`；生产路径上 kv `cloud_url` 在环境变量缺席时生效，环境变量优先，非法 URL 被拒 |
| 客户端 | 阶段 C 前不必改拨号测试。文案改动更新已经写死句子的测试：`ConnectionPresentationTest.kt`、`AccountScreenTest.kt`，以及桌面 `l10n.test.ts` 里 “Reachable from anywhere through Cloudflare”。再给称呼加一条：官方主机 / 其他主机 / 无 cloud |
| 端到端 | 阶段 A 用开发 host + iOS Debug 模拟器。阶段 B 用桌面 `npm run dev` 填地址，再断 Wi‑Fi 只留蜂窝或另一网络。官方三档路线各走一次：自动、先 cloud、只用直连 |

回归时官方 cloud 的账号、推播、webhook 不在私有 URL 上测。测它们时清空 `cloud_url`。

服务器上确认日志文件里没有 `f` 的 `d` 字段、没有 inner JSON。`hello` 的明文字段也不要进日志。

## 13. 跟随上游

`private-relay/src/room.ts` 顶部注明它对照的协议章节和 `cloud/src/relay.ts` 的职责。上游改 §7 的消息或关闭码时，先改私有服务的测试，再改房间逻辑。

Host 和 App 的 diff 应长期只停留在 `cloud.rs` 的 URL 来源、`api/remote.rs` 与 `devices.rs` 的方法登记、CLI、桌面地址栏和三端称呼。一旦发现要改 `crypto.rs` 或 `ChannelTransport` 的拨号，就是协议分叉了，停下来对照 §7，而不是在客户端加 `if (hongKong)`。

切回官方：清空 `cloud_url`，打开 cloud 开关。没有迁移脚本，也没有第二套身份。

## 14. 风险

| 风险 | 处理 |
| --- | --- |
| 私有服务漏做 `acl.ok`，或签的是规范化后的 JSON，或 `badAcl` / `staleVersion` 没带 `ver` | Host 不会打出 `relay ready`。缺 `ver` 时 host 把它当成 0，不会抬过中继上已有的版本，重发一次后放弃。用向量和「字节级验签」测试卡住 |
| Nginx 开了 http2，或 `Connection` 无条件写成 `upgrade`，或超时仍是 60 秒 | WebSocket 升级失败，或每分钟断线。配置按 §10 |
| 验签用了 `request.url` 的 `127.0.0.1:8788`，或 `Host` 带上 `:443` | 全部 `401 badSignature`。Node 用来验签的 authority 必须等于 `cloud::authority` 对这个 base 的结果 |
| 切换 URL 时电脑仍在官方账号里 | 先解除账号再切。空 grants 的成功拉取会立刻删掉账号来源的手机。切走之后再 unclaim，打到的是私有中继的 404，官方 D1 记录还在 |
| 复制 `identity.json` 到第二台机器 | 官方行为：后连上的 host 把前一条挤掉（`4009`），本机等 5 分钟。私有中继同样如此 |
| 推播和 TURN 仍经过 Cloudflare | 在设置文案里说清楚。第一期的隐私收益是聊天管道和 mailbox 的元数据，不是全部网络关系。中继仍看得到 §3.4 列出的握手明文 |
| Webhook URL 会显示成 VPS 地址但返回 404 | 阶段 D 之前，私有 cloud 上不要发公开 webhook；需要 webhook 时切回官方 cloud |
| 上游 Durable Object 增加字段 | 忽略未知 `t`（host 已经这样做）。新增的必选字段要在阶段说明里更新，否则只表现为某项能力缺失，而不是解密错误 |
| 把 `http://[::1]` 或 Android 不接受的 http 写进二维码 | 整张码作废，或 Android 的 hello 失败。给手机的地址用公网 https |

## 15. 实现时要打开的文件

| 目的 | 路径 |
| --- | --- |
| 线协议 | `docs/reference/remote-relay.md` §4–§7，文末 clarifications |
| 官方房间 | `cloud/src/relay.ts`、`cloud/src/routes/relay.ts`、`cloud/src/auth.ts`、`cloud/src/routes/host.ts` |
| Host 出站 | `host/src/remote/relay.rs`、`host/src/remote/cloud.rs`、`host/src/remote/channel.rs` |
| `setCloud` 的登记处（`setCloudUrl` 加在旁边） | `host/src/api/remote.rs`、`host/src/api/devices.rs` 的 `LOCAL_ONLY`、`host/src/main.rs` 的 `cloud` 子命令、`host/src/analytics/observe.rs` |
| 二维码和直连地址 | `host/src/service/mod.rs`、`host/src/api/remote.rs` 的 `pairing` |
| 服务环境变量写了什么 | `host/src/service/unix.rs`、`host/src/service/windows.rs` |
| 手机拨号 | `apps/ios/Kit/Sources/CodyncKit/Client/ChannelTransport+Connection.swift`、`apps/android/core/src/main/kotlin/com/codync/android/core/HostConnector.kt` |
| 手机是否接受这个 cloud | `apps/ios/Kit/Sources/CodyncKit/Client/Pairing.swift`、`apps/android/core/src/main/kotlin/com/codync/android/core/Computer.kt` |
| 路线和连接文案 | `apps/ios/Views/SettingsView.swift`、`apps/ios/Views/ComputerStatus.swift`、`apps/ios/Views/PairingView.swift`、`apps/ios/Kit/Sources/CodyncUI/Thread/ThreadView.swift`、`apps/android/app/src/main/kotlin/com/codync/android/SettingsView.kt`、`apps/android/app/src/main/kotlin/com/codync/android/ConnectionPresentation.kt`、`apps/android/app/src/main/kotlin/com/codync/android/PairingView.kt`、`apps/desktop/src/renderer/views/settings/ComputersView.tsx` |
| 翻译键 | `apps/ios/Kit/Sources/CodyncKit/Resources/Languages/`（桌面 `l10n.ts` 读这里） |
| 画面 TURN | `host/src/remote/cloud.rs` 的 `screen_ice`、`host/src/screen/viewer.rs` 的 `IceConfig` |
| 推播 URL（不要随 cloud 改） | `apps/ios/Kit/Sources/CodyncKit/Client/SharedStore.swift`、`apps/android/app/build.gradle.kts`、`host/src/remote/push.rs` |
| 向量 | `docs/reference/fixtures/remote-relay-vectors.json` |
