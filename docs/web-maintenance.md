# 保守メモ（Web 版リリース手順）

CLI / ライブラリ側の保守は従来どおり。本書は Web 版（`web/`）の
リリースと運用の決まりだけを扱う。

## バージョン規則（最重要）

**バージョンの正は GitHub のタグ `vX.Y.Z`。**
`web/package.json` の `appVersion` は、タグから `v` を除いたものと**必ず一致**させる。
上げるのは**タグを切る前**。

ビルド時、`appVersion` が成果物へ刻印される：

- `index.html` の `%APP_VERSION%` 置換（ヘッダーのバージョン表示）
- `dist/version.json` に `{"version": "X.Y.Z"}` を発行（アプリの更新チェック用）
- `dist/sw.js` のキャッシュ名 `pic2gray4px-X.Y.Z`

刻印は `web/assemble-site.mjs` が担う。プレースホルダが残っていたら
ビルドは失敗する（残存チェック付き）。

実機確認用は **v0.1.0 番代**、正式リリースは **v1.0.0 番代**
（Voice2QR / MiniMapPrint と同じ作法）。

## リリース手順

1. `web/package.json` の `appVersion` を次のタグに合わせて上げる
2. ローカルでビルドして刻印を確認する（後述のビルド手順。
   `web/dist/index.html` の表示、`dist/version.json`、`dist/sw.js` の
   キャッシュ名に新しい版数が入り、プレースホルダが残っていないこと）
3. 変更を main へ push する
4. GitHub Releases → Draft a new release → タグ `vX.Y.Z` を新規作成して Publish
   （タグの push で Pages ワークフローが起動し、タグのコミットから
   テスト → WASM ビルド → Web ビルド → 公開まで回る）
5. 公開先 https://tanep3.github.io/pic2gray4px/ を開き、ヘッダーの
   バージョン表示と `version.json` が新しいことを確認する

## 初回だけの設定（GitHub 側）— 2026-11-16 設定済み

1. Settings → Pages → Source を **GitHub Actions** にする
2. Settings → Environments → github-pages → Deployment branches and tags を
   **Selected branches and tags** にし、タグのルール `v*` を追加する
   （これを忘れるとタグからのデプロイが保護ルールで拒否される）
3. `.github/workflows/pages.yml` と `web/public/icons/icon-192.png` /
   `icon-512.png` をリポジトリへ配置する
   （ワークフローファイルとバイナリは GitHub コネクタで push できないため
   Web UI から手作業で上げた）

## ビルド手順（ローカル）

前提：Rust + wasm-pack、Node。

```sh
cd web
npm run wasm    # wasm-pack build（出力先は絶対パス指定。後述の注意）
npm ci          # 初回のみ
npm run build   # tsc 型チェックは npm run typecheck
```

注意：wasm-pack の `--out-dir` は **crate（`wasm/`）基準で解決される**。
`web/package.json` の `wasm` スクリプトは `$PWD` で絶対パス化してある。
相対パスに戻すと生成物が `wasm/web/public/wasm` に落ちてサイトに載らない。
`web/public/wasm/` は生成物なので git 管理外（CI が毎回ビルドする）。

## キャッシュの決まり（Service Worker）

- Cache Storage はオリジン全体で共有される。削除してよいのは
  **自アプリの接頭辞 `pic2gray4px-` の旧版キャッシュだけ**。
  他アプリ（同じ `tanep3.github.io` 配下の Voice2QR 等）のキャッシュを
  消すと相手のオフライン動作が壊れる。
- 例外：`pic2gray4px-share`（共有ターゲットの郵便受け）は版キャッシュの
  掃除対象から**常に外す**。消すと遷移中の共有画像が消える。

## 共有ターゲット（受信）の仕組み

静的ホスティングは POST を受けられないため、Service Worker が
`./share-target` への POST を横取りする：

1. 画像を `pic2gray4px-share` キャッシュへ
   `shared-image/<UUID>` キーで保存し、保存時刻を応答ヘッダ
   `x-pic2gray4px-saved-at` に持たせる
2. `./?share=<UUID>` へ 303 リダイレクト
3. アプリは起動時にそのキーを取り出して**1回だけ消費**（読み取り後に削除）
4. 取り出されなかった残骸は、起動時の掃除で **10分以上経過した分だけ**
   削除する（有効期限方式）。「今回分以外を全部消す」方式は採らない
   ——別タブがこれから取り出す共有まで消してしまうため

## 印刷の幅について

印刷時は結果画像だけを、物理幅で出す。幅は出力ピクセル数 ÷ 想定 DPI で
計算する：576 プリセットは 300dpi 機（M02S 級）、それ以外は 203dpi 機
（T02 級）として計算する（`web/src/main.ts` の印刷ハンドラ）。
プリセットと実機がずれる使い方では幅もずれるので、その場合は PNG を
保存して印刷アプリ側で合わせるのが確実。

## 実機確認

確認は Pages 上の実機で行う（要件 `docs/web-requirements.md` §8）：
共有での受け取り、貼り付け、EXIF の向き、12MP 級の変換時間と
メモリ、PWA インストール、共有での Phomemo アプリへの受け渡し。
