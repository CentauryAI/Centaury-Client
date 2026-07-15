# centaury 圖

厚客戶端，僅連 agent 於 kore 雲。Crate: `centaury`。倚 `kore-protocol`（共享型）。

## 檔案關係

```
main.rs ──┬─→ config.rs (token/url/http_client)
          ├─→ ws.rs     (connect/listen/next_delivery)
          ├─→ hook.rs   (Claude Code hook, 倚 config+ws+main::do_register)
          └─→ commands/send.rs (倚 config+ws)

examples/bench.rs — 獨立, 只倚 reqwest+tokio, 不倚他 mod
```

`do_register` 定義 main.rs（pub async fn), register 令 與 hook 自動註冊 共用之。

## main.rs

Cli 用 clap derive。Commands 列舉:
- `Register {name, project, sector, tool, human, owner}` — 呼 `do_register`, 存 token
- `Projects` — GET `/v1/projects`
- `Send(SendArgs)` — 轉 `commands::send::run`
- `Listen` — 轉 `ws::listen`
- `List` — GET `/v1/instances`, 印 name/status/owner/sector/tool/doing
- `History {limit, thread}` — GET `/v1/messages`
- `Unregister {name}` — DELETE `/v1/instances/{name}`
- `Status {text}` — PATCH `/v1/instances/self`
- `Hook(Claude|Install{user})` — 轉 hook.rs

`get_json<T>` 泛型 helper: GET + bearer token + deserialize，各 command 共用。

## config.rs — token 與 url 解

- `kore_dir()`: `$KORE_DIR` 或 `~/.kore`
- `token_path_for(project)`: 全域 agent → `$KORE_DIR/token`；project agent → `$KORE_DIR/<project>/token`
- `token_path()`: 解序 `KORE_TOKEN_FILE`（顯式覆蓋，一機多 agent 各自 identity）> `KORE_PROJECT` 域 path > 全域
- `server_url()`: `KORE_SERVER_URL` 或 `http://localhost:8080`
- `reg_secret()`: 須 `KORE_REG_SECRET`（org admin 給）
- `http_client()`: reqwest client, timeout 30s（伺服器卡死不致卡調用 agent 之 turn）
- `ws_url()`: server_url 轉 ws/wss scheme, join `/v1/ws`
- `save_token` / `load_token`: 存/讀 token file
- `instance_name()`: 解 JWT payload 之 `sub`（僅顯示用，非驗證——伺服器才驗）

## ws.rs — WebSocket 層

- `WsStream` type alias
- `connect(peek: bool)`: 開已驗證 socket；`peek=true` 觀察即時訊息不消（無 replay，watermark 不動）
- `CLOSE_REPLACED = 4000`: 伺服器 close code，義同一 identity 新連線頂替舊連線
- `next_delivery(ws)`: 讀下一 Delivery frame，跳非 text frame；遇 CLOSE_REPLACED 回 Err("replaced: ...")
- `listen()`: 外層 loop，指數 backoff（1s→60s上限），遇 "replaced:" 錯即 exit(1)（一 identity 僅容一 listener）
- `listen_once()`: connect(false) 一次連線壽命，印每則訊息（伺服器連線時自動 replay 漏失訊息）

## hook.rs — Claude Code hook 整合

kore 之 push 側。伺服器即 inbox（watermark+replay），故 hook 薄：連 socket，drain 伺服器 replay/push 者，交予 agent。

三事件：
- `Stop`: 阻塞至 `KORE_HOOK_TIMEOUT`（預設120s）候訊息；到則回 `{"decision":"block","reason":<msgs>}` exit(2)，turn 續（legacy 行為）
- `UserPromptSubmit`: 快速 drain（~1s），注入 additionalContext
- `SessionStart`: 若無 token 先 `auto_register()`；印 bootstrap context 告知 agent 用法

生成型 agent 無人手動 register：token 缺/被拒 → hook 自 register（`KORE_NAME` 或衍生穩定名）重試一次。

- `auto_name()`: 解序 `KORE_NAME` > 現存 token 之 name > hash(host+cwd+project) 衍生 `agent-XXXXXX`（確定性，重試不重複註冊）
- `auto_register()`: 讀 `KORE_PROJECT`/`KORE_OWNER`，呼 `crate::do_register`
- `post_status(text)`: fire-and-forget PATCH status，失敗不影響 hook
- `connect_or_register()`: 無 token 先 register；connect 失敗且有 reg_secret → register 重試一次
- `stop_poll()`: 首則等 timeout，其後 `drain_into` 批量吸收 burst（DRAIN_WINDOW=500ms），格式化後 exit(2)
- `prompt_drain()`: 同 drain，無 timeout 等待首則（有即報，無即靜默過）
- `drain_into(socket, out)`: loop 以 DRAIN_WINDOW 逐條吸收已到達 delivery
- `format_messages()`: 組訊息文字，含 thread/reply_to meta，尾附回覆指引
- `KORE_SKILL`: 內嵌 skill 內容（教 agent 用 centaury 之指令），install 時寫入 `.claude/skills/kore/SKILL.md`
- `install(user_scope)`: 合併 hooks 進 `.claude/settings.json`（Stop timeout 86400，UserPromptSubmit/SessionStart 各30），寫 skill 檔

## commands/send.rs

`SendArgs`: positionals（`@target` 或裸字）、`message`（`--` 後）、`--stdin`、`--file`、`--base64`、`--intent`、`--thread`、`--reply-to`、`--wait`、`--timeout`（預設60）。

- `process_positionals()`: 拆 `@target` 與裸字；單一裸字含 `@`+空格 → legacy「整段內嵌 @mention」相容
- `resolve_message_text()`: 四來源（`--`/`--stdin`/`--file`/`--base64`）僅容其一；`--` 後有裸字則報錯（防靜默 broadcast）；裸字恰一則用之，多則報錯促用 `--`；皆無且非 tty 則讀 stdin
- `run(args)`: 解 targets/text → POST `/v1/messages`。`--wait` 者，**先**開 peek socket 再送（防快速回覆漏接於間隙），send 後於 peek socket 候 `reply_to == 本訊息id` 之 delivery，逾 timeout 報錯並提示 `--reply-to` 補救

## examples/bench.rs

送壓測工具。Usage: `bench <server-url> <token> <concurrency> <seconds> [message]`。跑伺服器須 `KORE_RATE_LIMIT=0`（否則限流擋之）。並發 task 各自 loop POST 至 deadline，收集 latency，末印 p50/p95/p99/max + rps + error count。不倚本 crate 他 mod（獨立二進位）。

## 常見改動指路

- 加新 CLI 子命令 → `main.rs` Commands enum + match 分支
- 改 token/url 解序 → `config.rs`
- 改連線/重連邏輯 → `ws.rs`
- 改 hook 行為（Stop/UserPromptSubmit/SessionStart）→ `hook.rs`
- 改 send 之 flag/驗證 → `commands/send.rs`
- 協定型別（Delivery/SendRequest等）不在此 crate，見 `protocol/src/api.rs`
