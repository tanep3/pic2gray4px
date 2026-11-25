# pic2gray4px Web 設計書

要件は `docs/web-requirements.md`。本書はその実装設計であり、
冒頭に通しの動作例、続いて関数・メッセージの具体的な表面を置く。

## 1. 通しの動作例

Android の写真アプリで写真（4032×3024、EXIF 回転あり）を「共有」→
pic2gray4px Web を選んだ場合（iOS は共有ターゲット未対応のため、
貼り付け経由になる。要件 §3.1）：

1. 共有 POST（multipart/form-data）が飛び、Service Worker が画像を受けて
   一時キャッシュへリクエストごとの識別子付きキーで置き、
   `./?share=<識別子>` へ 303 リダイレクトする。
2. アプリが起動し、識別子に対応する画像を一時キャッシュから取り出して
   即座に削除する。
3. 画像 Blob を Worker へ渡す。Worker が EXIF 向きを反映してデコードし、
   RGBA（4032×3024）を取り出す。
4. 既定サイズは原画幅。ユーザーがプリセット「384」を選ぶと世代番号が進み、
   `convert_rgba_adjusted(rgba, 4032, 3024, options{ targetWidth: 384,
   contrast: null, unsharp: null, passes: 10, mode: 'photo', ... })`
   が Worker 内で走る。
5. コアは Lanczos3 で 384×288 へ縮小 → 自動調整（例：contrast 1.08 /
   unsharp 0.5 を選択。自動調整の候補は contrast が 1.0／1.04／1.08／1.12、
   unsharp が 0／0.25／0.5／0.75／1.0／1.25／1.5）→ 変換し、
   `{ width: 384, height: 288, pixels,
   contrast_used: 1.08, unsharp_used: 0.5 }` を返す。
6. 続けて `encode_1bit_png(384, 288, pixels)` で 1-bit PNG のバイト列を作る。
7. 画面には原画と変換結果が並び、適用値と寸法が表示される。
   「共有」から Phomemo アプリへ PNG を渡す。

## 2. 全体構成

```text
pic2gray4px/            既存リポジトリ（CLI は無変更で存続）
  src/                  既存 Rust ライブラリ（正本）＋ §3 の追加2関数
  wasm/                 新規 crate: wasm-bindgen の薄い皮（§3.3）
  web/                  新規: Vite + 素の TypeScript（フレームワークなし）
    src/                main / input / client / worker / export / i18n / pwa
    public/             manifest, sw.js, icons, wasm/（ビルド時に生成）
  .github/workflows/pages.yml   タグ駆動（要件 §6。アップロードは手作業）
```

境界（要件 §4 の実装）：

- ブラウザ（TS）：入力の受け取り、デコード依頼、表示、出力（保存・共有・印刷）。
- Worker（TS + WASM）：デコード、RGBA 取り出し、コア呼び出し、PNG 化。
- コア（Rust）：Lanczos3 リサイズ、変換、1-bit PNG エンコード。結果を決める処理は全部ここ。

## 3. Rust 側

### 3.1 既存ライブラリへの追加（2関数のみ）

既存の表面（確認済み）：

- `ConversionOptions { width: Option<u32>, backend: Backend = Cell,
  passes: usize = 10,
  contrast: Option<f32>, unsharp: Option<f32>, diffusion: bool = true,
  exchange: usize = 0, mixer / tone_curve / local_contrast（色調整） }`
  - モード解決 `mode_preset("dither" | "photo" | "illustration") ->
    ModePreset { backend, diffusion, exchange }` もコアが持つ（名前表が正本）。
  - `contrast: None` = 自動、`Some(1.0)` = オフ。`unsharp: None` = 自動、`Some(0.0)` = オフ。
- `convert_with_options(&DynamicImage, ConversionOptions) -> Result<ConversionResult>`
  - 範囲検証（contrast 0〜3、unsharp 0〜2、passes 0〜10、width ≥ 1）→
    RGBA 化 → Lanczos3 リサイズ（縦横比維持）→ 奇数寸法パディング → 変換。
- `ConversionResult { width, height, pixels: Vec<u8>（画素ごと 0/1）, contrast, unsharp }`
- `write_png(path, w, h, pixels, overwrite)` — MSB 詰め＋
  `png::Encoder`（Grayscale / BitDepth::One / Compression::High）。

feature 分離（M1 で実施）：形式デコーダ群と `load_image` は feature
`load-image`（デフォルト有効）の後ろに置く。変換コアと 1-bit PNG
エンコードはデコーダを必要としないので、wasm crate は
`default-features = false` で依存し、WASM の依存ツリーにデコーダを
入れない。CLI はデフォルト feature のまま無変更。なお WASM の実サイズは
分離前後でほぼ変わらなかった（未使用のデコーダはリンク時に除去されて
いたため）——分離の目的はサイズではなく、依存の表面を正しく保つこと。

追加する関数：

| 関数 | 引数 | 戻り値 | 備考 |
| --- | --- | --- | --- |
| `convert_rgba` | `rgba: &[u8]`, `width: u32`, `height: u32`, `options: ConversionOptions` | `Result<ConversionResult>` | 長さが `width*height*4` でなければエラー。`RgbaImage` を作って `convert_with_options` へ委譲するだけ |
| `encode_1bit_png` | `width: u32`, `height: u32`, `pixels: &[u8]` | `Result<Vec<u8>>` | `write_png` の詰め＋エンコード部分をメモリ書き出しに切り出したもの。`write_png` はこれを呼んでファイルに書く形にリファクタし、ファイル保護（create_new / overwrite）の意味は従来どおり `write_png` 側に残す |

CLI の挙動は変えない（既存テスト全通過＋ §6 の一致テストが担保）。

### 3.2 一致テスト（要件 §4 のテスト条件）

`src/lib.rs` のテストに追加：

1. 合成 RGBA フィクスチャ（グラデーション＋図形、透過画素含む）について、
   `convert_rgba` 経由と `DynamicImage` 経由の `ConversionResult` が
   画素・寸法・適用値まで完全一致すること。
2. 同じ `pixels` について `encode_1bit_png` のバイト列と、
   `write_png` が書いたファイルの中身が完全一致すること。

### 3.3 wasm crate（`wasm/`）

- crate 名 `pic2gray4px-wasm`、cdylib。依存は
  `pic2gray4px = { path = "..", default-features = false }` と
  `wasm-bindgen` のみ。ロジックは持たず、型の詰め替えだけを行う。
- `[package.metadata.wasm-pack.profile.release] wasm-opt = false`
  （MiniMapPrint の前例。環境依存のダウンロードを避け再現性を取る）。

公開関数：

| 関数 | 引数 | 戻り値 | エラー |
| --- | --- | --- | --- |
| `convert_rgba` | `rgba: &[u8]`, `width: u32`, `height: u32`, `target_width: Option<u32>`, `contrast: Option<f32>`, `unsharp: Option<f32>`, `passes: u32`, `diffusion: bool` | `ConvertResult`（下記） | ライブラリのエラー文字列をそのまま `JsError` に載せる（範囲検証など） |
| `convert_rgba_adjusted` | 上記の引数に続き `mixer_preset_name: Option<String>`, `mixer_weights: Option<Vec<f32>>`, `tone_curve_name: Option<String>`, `local_contrast: Option<f32>`, `mode_name: Option<String>`, `exchange_rounds: Option<u32>` | `ConvertResult`（下記） | 同上。名前と値の同時指定（ミキサー、モード）はエラー。`mode_name` がある場合 `diffusion` 引数は無視される |
| `encode_1bit_png` | `width: u32`, `height: u32`, `pixels: &[u8]` | `Vec<u8>`（PNG バイト列） | 同上（寸法と画素数の不一致など） |

`ConvertResult` は `#[wasm_bindgen]` 構造体で、getter のみ：
`width: u32` / `height: u32` / `pixels: Vec<u8>`（画素ごと 0/1） /
`contrast_used: f32` / `unsharp_used: f32` / `mixer_used: Vec<f32>` /
`tone_curve_used: String` / `local_contrast_used: f32` /
`diffusion_used: bool` / `exchange_used: u32`。

注意（UI 側の写像）：上級者設定の「オフ」は CLI と同じく
contrast = `Some(1.0)`、unsharp = `Some(0.0)` に写像する。「自動」は両方 `None`。

## 4. Web 側モジュール

| ファイル | 役割 |
| --- | --- |
| `src/main.ts` | 画面の統括。状態機械（待機／変換中／完了／エラー）と結果の画面反映（世代の管理は client.ts） |
| `src/input.ts` | 入力の正規化：ファイル選択、ドラッグ＆ドロップ、貼り付け（§5.2）、共有受け取り（§5.3）をすべて `Blob` に揃える |
| `src/client.ts` | Worker のクライアント。世代番号の付与と**送信制御**：Worker 実行中は要求を送らず最新1件だけを保持し、完了（result / error）時に送る。最新世代の応答だけを解決し、捨てられた世代の Promise はキャンセルとして完了させる（§5.1） |
| `src/worker.ts` | Worker 本体。WASM の初期化、デコード、変換、PNG 化（§5.1） |
| `src/export.ts` | 保存（PNG ダウンロード）、共有への出力（Web Share API、不可なら保存）、印刷（印刷 CSS） |
| `src/i18n.ts` | 日英辞書。初期値は端末言語 |
| `src/pwa.ts` | Service Worker 登録、`version.json` 照合による更新通知（Voice2QR の作法） |

## 5. 処理の詳細

### 5.1 変換パイプラインと Worker プロトコル

メイン → Worker：

| type | フィールド | 意味 |
| --- | --- | --- |
| `init` | `wasmBase: string` | WASM 一式のある絶対 URL。Worker は glue を動的 import し初期化する（モジュール相対で解決しない。MiniMapPrint の教訓） |
| `convert` | `gen: number`, `blob: Blob` または `rgba: ArrayBuffer, width, height`, `options: { targetWidth: number \| null, contrast: number \| null, unsharp: number \| null, passes: number, mode: string, mixerPreset: string \| null, toneCurve: string \| null, localContrast: number }` | 変換要求。`contrast: null` = 自動（オフは UI が 1.0 に写像済み）。`targetWidth: null` = リサイズなし。原画プリセットは `null` を渡し、原画幅の数値を入れて同じ寸法への Lanczos3 を走らせないこと。`mode` はコアの名前表で解決される |

Worker → メイン：

| type | フィールド | 意味 |
| --- | --- | --- |
| `ready` | `workerDecode: boolean` | 初期化完了。Worker 内デコード（createImageBitmap + OffscreenCanvas）が使えるか |
| `result` | `gen`, `width`, `height`, `pixels: ArrayBuffer`（transferable）, `png: ArrayBuffer`（transferable）, `contrastUsed`, `unsharpUsed`, `sourceWidth`, `sourceHeight` | 変換結果。プレビュー描画と保存・共有はこの2バイト列で足りる |
| `error` | `gen`, `message: string` | 失敗。表示文言への写像はメイン側（i18n） |

規則：

- デコードは原則 Worker 内（Blob をそのまま渡すので原寸 RGBA の転送が起きない）。
  `workerDecode = false` の環境では、メインスレッドでデコードして
  RGBA を transferable で渡す（フォールバック経路を仕様として持つ）。
- Worker は要求を直列に処理するだけとし、要求の集約はメイン側
  client.ts の送信制御で行う（§4）。理由：WASM の同期処理中は Worker
  自身の JS もメッセージを処理できないため、Worker 側で
  「最後の1件だけ保持」は保証できない。client.ts は実行中は次の要求を
  送らず最新1件だけを保持し、完了時に送る。実行中の変換自体は
  止められない（WASM は同期的）ため、その結果は世代チェックで捨てる。
- 捨てられた世代の Promise は未解決のまま残さない。キュー中（未送信）の
  ものは新しい要求が来た時点で即座に、実行中のものは結果到着時に、
  それぞれ「キャンセル」として完了させる（未処理の rejection を作らない
  形で）。呼び出し側はキャンセルを「何もしない」で扱い、待機が残らない
  ようにする。
- メインは `gen` が現在の世代と一致する `result` / `error` だけを画面に反映する。
- EXIF 向きは `createImageBitmap(blob, { imageOrientation: "from-image" })` で反映する。

### 5.2 貼り付け入力

経路は `paste` イベントの1本だけ（Voice2QR v1.2.2 で確定した形に準拠、
2026-11-16 に要件 §3.1 も改定）。

- PC：Ctrl+V / Cmd+V の `paste` イベントで `clipboardData` の画像を受ける
  （`files` 先頭→ `items` の `getAsFile()` の順に探す）。画像を含まない
  貼り付けは手を付けない（入力欄の通常動作を保つ）。
- スマホ：編集可能な「貼り付け受け箱」（`contenteditable`、プレースホルダ
  「画像を貼り付け」を CSS で表示）をタップ → OS の呼び出しメニューの
  「ペースト」。ページ側から能動的に読み取ることはできない。受け箱は
  編集対象ではなく受け口なので、ブラウザが挿入した中身はハンドラが
  直後に消去する。
- Async Clipboard API（`navigator.clipboard.read()`）は使わない。その画像の
  表面は実質 PNG のみで、他アプリが JPEG で置いたコピーは到達しない
  （iPad で X からコピーした画像で実測、Voice2QR 側の知見）。
  `paste` イベントの従来経路は JPEG 等も通す。

### 5.3 共有ターゲット（受信）

- `manifest.webmanifest` の `share_target`：
  `action: "./share-target"`, `method: "POST"`,
  `enctype: "multipart/form-data"`,
  `params.files: [{ name: "image", accept: ["image/*"] }]`。
- `sw.js` の fetch ハンドラが `./share-target` への POST を横取りする：
  `formData()` から `image` を取り、リクエストごとの識別子
  （`crypto.randomUUID()`）を発行して、Cache Storage `pic2gray4px-share` に
  `/shared-image/<識別子>` として保存し、`./?share=<識別子>` へ
  303 リダイレクトを返す。固定キーだと短時間に複数の共有が来た場合に
  後の画像が前の画像を上書きするので、キーとリダイレクト先の両方に
  識別子を持たせる。
- アプリは起動時に `share=<識別子>` を見たら同キャッシュを開いて
  そのキーの画像を取り出し、**読み取り後に必ず削除**する。
  通常の入力（Blob）として変換フローに入る。取り出されなかった共有の
  掃除は**有効期限方式**にする：保存時に応答ヘッダへ保存時刻を持たせ、
  起動時の掃除では一定時間（例：10分）以上経過したエントリだけを削除する。
  「今回分以外を全部消す」方式は採らない（並行する別の共有——別タブが
  これから取り出す画像——まで消してしまうため）。
- キャッシュ掃除の注意：バージョン更新時のキャッシュ削除は
  `pic2gray4px-` 接頭辞のうち自アプリの版キャッシュだけを対象とし、
  `pic2gray4px-share` は対象から外す（消すと共有画像が遷移中に消える）。

### 5.4 PWA / Service Worker

- アプリ本体のキャッシュ名は `pic2gray4px-<appVersion>`。ビルド時に
  sw.js 内の版数プレースホルダを置換する（casshern_sim で導入した刻印と同じ仕組み）。
- ナビゲーションは network-first、静的資産は cache-first（版付きキャッシュ前提）。
- 有効化時に自接頭辞の旧版キャッシュだけを削除する（§5.3 の共有キャッシュを除く）。

## 6. 画面構成

モバイル単列を基本とし、PC ではプレビューを横並びにする。

1. ヘッダー：ロゴ＋アプリ名、言語切替、バージョン表示。
2. 入力エリア（ヒーロー）：ドラッグ＆ドロップ枠を大きく置き、
   貼り付け受け箱を並べる。ドロップ枠自体がクリックでファイル選択を
   開くため、「ファイルを選ぶ」ボタンは置かない（v0.3.0 で廃止）。
3. サイズ選択：チップ列 — 原画（初期値）／384（T02等）／576（M02S等）／
   250（E-ink）／カスタム（数値入力）。
4. モード選択：写真／イラストのチップ列（初期値は写真）。
   色調整（ミキサー／トーンカーブのチップ列）がその下に続く。
5. プレビュー：原画カードと結果カード。結果は `image-rendering: pixelated`、
   タップで拡大モーダル。結果カードの下に適用値（contrast / unsharp の実値、
   モード名、出力寸法）を出す。
6. アクション：共有／保存／印刷。
7. 上級者設定（`<details>` で畳む）：contrast（自動／オフ／スライダ 0〜3）、
   unsharp（自動／オフ／スライダ 0〜2）、passes（スライダ 0〜10）、
   局所コントラストの強さ（スライダ 0〜1）。拡散トグルはモードに
   包含されるため置かない（v0.3.0 で廃止）。
   変更すると再変換が走る。

デザインは Voice2QR と同じ方向性（ゆめかわ／エモ）。色・角丸・装飾の
トークンは web-design 実装時に style.css へ集約し、部品ごとに散らさない。

## 7. エラー表

| 場面 | 条件 | 画面の扱い |
| --- | --- | --- |
| デコード | 対応形式でない／壊れている | 「この画像は読み込めませんでした」 |
| 貼り付け | 画像を含まない貼り付け | 何もしない（通常の貼り付けとして扱う。受け箱の中身は消去） |
| 共有受け取り | 一時キャッシュに画像が無い | 通常起動として扱い、メッセージは出さない |
| 変換 | ライブラリの範囲検証エラー等 | エラー文字列を結果エリアに表示（開発時）／利用者向け文言に写像（公開時） |
| WASM 初期化 | glue / wasm の取得失敗 | 「読み込みに失敗しました。再読み込みしてください」 |

## 8. ビルドと Pages

- ビルド順：`wasm-pack build wasm/ --target web --out-dir ../web/public/wasm`
  → `web/` で `npm ci` → `npm run build`（tsc + Vite）。
- バージョン刻印：`web/package.json` の `appVersion` を index 表示、
  `version.json`、sw.js のキャッシュ名へ反映する（要件 §6）。
- Pages ワークフローはタグ駆動（`v*` ＋手動実行）。ワークフローファイルは
  コネクタで push できないため、実装時にファイルを渡して手作業で上げてもらう。
- ローカル確認は Drive 経由のテスト zip → 実機、の従来ループ（Voice2QR の作法）。

## 9. テスト

- Rust（ネイティブ）：既存テスト＋ §3.2 の一致テスト2件。`cargo test`。
- WASM スモーク（node）：パッケージを読み、グラデーションのフィクスチャを
  変換して白率が妥当なこと、`encode_1bit_png` の出力が PNG 署名を持ち
  IHDR のビット深度 1・色種 0 であることを確認する。
- ブラウザ：実機確認項目は要件 §8 に従う（EXIF、12MP 計測、共有入出力、PWA）。

## 10. 実装順序

1. M1 コア：ライブラリ追加2関数＋一致テスト、wasm crate、WASM スモーク。
2. M2 Web 骨格：Worker パイプライン、ファイル入力、サイズ選択、プレビュー、
   保存。PWA 外枠（manifest / sw / 刻印）。
3. M3 入力の完成：ドラッグ＆ドロップ、貼り付け、共有ターゲット受信。
4. M4 仕上げ：エモいデザイン、i18n、共有への出力、印刷。
5. M5 リリース準備：pages.yml、保守ドキュメント（web-maintenance）、
   実機確認（v0.1.0 番代）→ 正式 v1.0.0。
