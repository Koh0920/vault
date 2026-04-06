# BYOK Encrypted R2 Drop

このサンプルは、レビューを反映して技術スタックを `browser + presign broker` から `Tauri + rclone sidecar + crypt + RC API` に更新した設計メモです。

対象は `cloud-first web app` ではなく、`local-first multi-backend backup capsule` です。Google Drive、R2、Backblaze などを同じ操作面で扱いながら、暗号化と大容量転送をローカル側に閉じ込めることを主目的にします。

## 採用モデル

- UI は HTML + TypeScript で構成する
- デスクトップ shell は Tauri を使う
- 転送エンジンは `rclone + crypt` を sidecar として同梱する
- rclone の制御は stdout parse ではなく RC API を優先する
- UI と backend の通信は localhost HTTP ではなく Tauri `invoke` / `event` を使う
- Google Drive OAuth や各種 backend credential は browser に渡さず、ローカルの rclone config に閉じ込める

## なぜこの構成か

この用途では、browser 単体で大容量暗号化アップロードを再実装するより、`rclone + crypt` の既存の強みをそのまま使う方が圧倒的に楽です。

主な理由は次の通りです。

1. Google Drive backend が成熟している
2. `crypt` でファイル名と内容の両方を暗号化できる
3. 10GB 級でも browser memory に載せずに済む
4. retry、resume、filter、sync を既存機能でまかなえる
5. 同じ backend を CLI / desktop / 将来の guest UI でも共有しやすい

## 破棄した案

- `browser + aws-sdk + presign broker` を本命にする
  - R2 first ならよいが、Google Drive 対応まで含めると backend ごとの差が大きい
  - OAuth、resume、filter、sync を個別実装するコストが高い
- `stdout/stderr` の progress parse で rclone を薄く包む
  - 最初は動くが、進捗、cancel、再試行、job 管理が脆い
- Electron shell
  - Node フル権限で重く、配布サイズも大きい
  - この用途では Tauri の方が軽く、sidecar 前提と相性がよい

## 更新後の技術スタック

### Frontend

- HTML
- TypeScript
- Vite
- 必要なら Tailwind

### Desktop Shell

- Tauri
- Rust command layer
- Tauri event stream for progress

### Transfer Engine

- `rclone`
- `crypt` remote
- `rcd` mode with RC API

### Local Persistence

- app 専用 `rclone.conf`
- OS keychain で機密値保護
- ジョブ履歴は SQLite か軽量 JSON store

## システム構成

### 1. Tauri Frontend

役割:

- バックアップ元の選択
- 保存先 provider の選択
- crypt password の入力
- ジョブ開始
- 進捗、速度、残り時間、失敗の表示

### 2. Tauri Backend

役割:

- sidecar として `rclone` を起動
- `rclone rcd` の lifecycle を管理
- RC API を叩く薄い wrapper を提供
- config path と keychain を管理
- フロントへ progress event を流す

### 3. rclone Sidecar

役割:

- Google Drive / R2 / Backblaze 等への転送
- `crypt` による暗号化
- multipart / resume / retry
- backend ごとの OAuth / token refresh

## アップロード手順

1. Tauri UI で source path と destination remote を選ぶ
2. 初回のみ provider 接続を作る
3. backend が app 専用 `rclone.conf` を用意する
4. backend が `crypt` remote を生成する
5. backend が `rclone rcd` を sidecar 起動する
6. UI が `startUpload` を呼ぶ
7. backend が RC API で `sync/copy` を `_async=true` で開始する
8. backend が `job/status` と `core/stats` を poll する
9. backend が Tauri event で progress をフロントへ送る
10. UI が完了 / 失敗 / cancel を表示する

## 推奨 API 面

UI から見える API は 5 個程度に絞るのがよいです。

### `getProviders()`

返すもの:

- 対応 backend 一覧
- `drive`, `r2`, `b2` など

### `connectProvider(provider)`

役割:

- OAuth backend の初回接続
- browser launch と callback 受け
- app 専用 `rclone.conf` への保存

### `createCryptRemote(request)`

役割:

- 平 remote を元に `crypt` remote を生成
- password は config 平文保存ではなく runtime 注入を優先

### `startUpload(request)`

```ts
 type StartUploadRequest = {
   sourcePath: string;
   remoteName: string;
   remotePath: string;
   mode: "copy" | "sync";
 };
 
 type StartUploadResponse = {
   jobId: string;
   executeId?: string;
 };
```

### `getJobStatus(jobId)`

```ts
 type JobStatus = {
   jobId: string;
   executeId?: string;
   phase: "preparing" | "running" | "verifying" | "done" | "failed";
   progress?: {
     bytesDone: number;
     bytesTotal?: number;
     speed?: number;
     eta?: number;
     currentFile?: string;
     transfers?: number;
   };
   error?: string;
 };
```

## RC API を優先する理由

`rclone copy --progress` のログ parse は避けた方がよいです。RC API なら JSON で job を扱えるので、次が楽になります。

- `job/list`
- `job/status`
- `core/stats`
- `job/stop`
- backend 再起動後の復元

このため、thin wrapper は `subprocess wrapper` ではなく `RC bridge` として作る方がよいです。

## config / secret 管理

- `~/.config/rclone/rclone.conf` は触らず、アプリ専用 config を使う
- 常に `--config <app-specific-path>` を付ける
- `crypt` password は可能なら keychain 保持、最低でも runtime 注入に留める
- Google Drive token は rclone が管理し、フロントエンドに露出させない

## MVP の責務

- 1 ファイル 10GB までを許容する
- Google Drive と R2 を最初の backend にする
- `copy` と `sync` をサポートする
- include / exclude は簡易入力だけ提供する
- ジョブ履歴はローカル保存のみにする

## セキュリティ境界

- frontend は filesystem / credential に直接触らない
- backend のみが `rclone.conf` と keychain に触る
- 転送はすべて sidecar だけが行う
- `crypt` の暗号化対象はファイル内容と名前の両方
- OAuth token は browser に渡さない

## 運用上の注意

- macOS builds では `src-tauri/tauri.macos.conf.json` で `src-tauri/bin/rclone-<target-triple>` を sidecar として同梱する
- backend は起動ディレクトリにある bundled launcher を優先し、見つからない場合は `RCLONE_SIDECAR_NAME` で指定した名前を `PATH` から解決する
- Tauri bundle に `rclone` launcher を arch ごとに含める
- sidecar バージョンを固定する
- RC API は localhost 固定で、認証を有効にする
- ジョブ状態は stdout parse ではなく RC stats を正とする
- app data 配下に config と log を分ける

## 将来拡張

- 同じ core を CLI カプセルへ共有
- guest UI から desktop host bridge 経由で操作
- backend 追加
  - S3
  - Dropbox
  - OneDrive
- backup profile 保存
- schedule 実行
- restore 導線

## 結論

このカプセルの本命は `html frontend + rclone + crypt` ですが、実装形としては `Tauri + rclone sidecar + RC API` が最もよいです。

つまり、見た目は HTML frontend、実体は local-first desktop capsule です。
