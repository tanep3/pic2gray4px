# 色調整 設計

要件は `docs/adjustment-requirements.md`。本書はその実装設計であり、
特に次の2点をここで確定する：

- トーンカーブの補間方式：**区分線形補間**
- 局所コントラストの増幅関数と半径の決定規則（§4）

## 1. 動作例

### CLI

```
$ pic2gray4px photo.jpg out.png --width 576 \
    --mixer warm --tone-curve contrast --local-contrast 0.5
out.png (576x432, 1-bit grayscale PNG; mixer=(0.50, 0.35, 0.15);
tone-curve=contrast; local-contrast=0.50; contrast=1.00 auto;
unsharp=0.00 auto; cell diffusion=on)
```

### コア呼び出し

```rust
let result = convert_with_options(&image, ConversionOptions {
    width: Some(576),
    mixer: Some((0.50, 0.35, 0.15)),   // None = standard
    tone_curve: ToneCurve::Contrast,   // 既定 Normal
    local_contrast: 0.5,               // 既定 0.0
    ..ConversionOptions::default()
})?;
assert_eq!(result.tone_curve, ToneCurve::Contrast);
```

## 2. コア API の変更

### ConversionOptions への追加

| フィールド | 型 | 既定 | 意味 |
|---|---|---|---|
| mixer | Option<(f32, f32, f32)> | None | None は standard の重みを意味する（省略＝標準） |
| tone_curve | ToneCurve | Normal | 列挙：Normal / Contrast / LiftBlacks |
| local_contrast | f32 | 0.0 | 0.0〜1.0。0 で処理自体をスキップ |

検証（既存の流儀でエラーを返す）：

- mixer の各要素が非有限、または −2.0〜2.0 の範囲外 → エラー
- local_contrast が非有限、または 0.0〜1.0 の範囲外 → エラー

### プリセットの名前解決

プリセット表はコアが持ち、名前解決もコアが行う：

```rust
pub const MIXER_PRESETS: [(&str, (f32, f32, f32)); 3];
pub fn mixer_preset(name: &str) -> Option<(f32, f32, f32)>;
impl ToneCurve { pub fn from_name(name: &str) -> Option<ToneCurve>; }
```

CLI / WASM は名前をそのままコアへ渡し、重みへの解決をコアに任せる
（名前表の正本をコア1か所にするため）。

### ConversionResult への追加

適用値として `mixer: (f32, f32, f32)`、`tone_curve: ToneCurve`、
`local_contrast: f32` を返す。CLI の出力行と Web の結果表示はこれを使う。

### WASM 表面

既存の `convert_rgba` は変更しない（色調整は全て既定となり、
従来結果とバイト一致する）。新しい関数 `convert_rgba_adjusted` を
追加する：

```
convert_rgba_adjusted(rgba, width, height, target_width, contrast,
                      unsharp, passes, diffusion,
                      mixer_preset_name,   // string | null
                      mixer_weights,       // Float32Array(3) | null
                      tone_curve_name,     // string | null
                      local_contrast)      // number | null
```

旧関数を残すのは互換性のため（古い JS からの呼び出しが、引数の
意味を変えずに従来通り動く）。Web は新しい関数へ移行する。
null は「省略」を表し、コアの既定が適用される。名前が不正なら
エラー文字列を返す。mixer_preset_name と mixer_weights の同時指定はエラー。

## 3. 処理の実装対応

新しい前段は新モジュール `src/adjust.rs` に置き、平面→平面の
純粋関数として実装する（単体テスト可能にするため）。

| 段 | 実装 | 備考 |
|---|---|---|
| ミキサー | `fill_luminance` の重みをパラメタ化 | 下記参照 |
| トーンカーブ | `adjust::apply_tone_curve(&mut plane, curve)` | §4 |
| 局所コントラスト | `adjust::apply_local_contrast(&mut plane, w, h, amount)` | §5 |
| 後段 | 既存の候補探索・セル最適化 | 変更なし |

`convert_with_options` 内の順序：

```
リサイズ → fill（ミキサー） → トーンカーブ → 局所コントラスト
→ eligible / tonal 判定 → 候補探索（基準は調整後の平面） → セル最適化
```

要件の「基準画像」ルールは、探索が読む target 平面そのものを
調整後の平面にすることで自然に満たされる。`has_tonal_range` や
`stats` も調整後の平面を読む。

### ミキサーの互換性

- 演算の形は現行と同一に保つ：`((w0*r + w1*g) + w2*b) / 255.0`
  （重みを変数化しても、乗算と加算の順序と 255 での除算は変えない）
- 重み付き和の後に 0〜1 クランプ、その後に現行と同じアルファ合成
  （`lum * alpha + 1.0 - alpha`）
- 標準重みでは現行式の値域がちょうど [0, 1] に収まることを確認済み
  （f32 実測：白 (255,255,255) で 1.0 ちょうど、最小は 0.0）。
  よってクランプは既定経路では無力であり、バイト一致を壊さない

## 4. トーンカーブ

- アンカー点は要件 §5 の表を定数として持つ
- 評価は画素ごとに区分線形補間する。LUT は使わない
  （平面は f32 の連続値であり、LUT 化は量子化誤差を持ち込む）
- アンカー範囲外の入力は端点の出力値に張り付く
- アンカー列の単調性はテストで保証する

## 5. 局所コントラスト

- 半径：`r = clamp(round(出力幅 / 64), 2, 24)`。
  384 → 6、576 → 9、250 → 4。印刷サイズに対して相対的に効くよう、
  出力幅に連動させる
- ベースは半径 r のボックスぼかし（2パスのスライディングウィンドウ）
- **境界処理**：窓は有効領域（valid_w × valid_h）の内側に切り取り、
  実際に含まれる画素数で割る（外側を何かで埋めない）。既存の
  アンシャープ（範囲外のタップを除き、残りの重みで正規化）と
  autotune の box_blur（窓を画像内に切り取り、実画素数で割る）と
  同じ規則に揃える
- パディング（偶数化の白マージン）は有効領域の外なので、ぼかしの
  入力にも調整の対象にもしない（1.0 のまま残る）。有効領域内の
  透明画素は、白合成後の平面値を持つ通常の画素として扱う
  （従来のパイプラインと同じ。統計と評価だけが eligible で区別する）
- ディテール `d = v − base` に対する増幅：

```
w(d) = 1 / (1 + (d / 0.15)^2)
out  = clamp01(v + amount * d * w(d))
```

  小さな偏差（質感）はほぼそのまま増幅され、大きな偏差（強いエッジ）
  では増幅率が落ちてハロを抑える。amount = 1 で質感の最大増幅は
  偏差の +100%
- amount = 0 のときは処理自体をスキップする（バイト一致の前提）
- 追加メモリ：ベース平面1枚（画素数×4B、400万画素で約16MB）＋
  行リングバッファ min(h, 2r+1) 行分。横パスは行ごとに base へ書き、
  縦パスはリングバッファ経由で base をインプレースに更新するため、
  中間の全面バッファは持たない。通常のアスペクト比ではリングは
  1MB 未満、極端な横長・縦長でも平面1枚分を超えない

## 6. CLI 設計

- 要件 §6 の4オプションを追加する。`--mixer` は名前を、
  `--mixer-weights` は "R,G,B" 形式を受け、解決はコアに任せる
- 同時指定・解釈できない値はエラーにする（既存の流儀）
- 出力行に適用値（mixer の重み、tone-curve 名、local-contrast）を追加する
- `--help` の文面に4オプションを追記する

## 7. Web 設計

- Worker の変換オプションに mixer / toneCurve / localContrast を追加し、
  `convert_rgba_adjusted` へそのまま渡す（省略は null）
- 選択 UI と上級者設定への配置は要件 §7 の通り。
  詳細は `docs/web-requirements.md` に追記する
- UI の初期選択は warm × contrast で、選択肢は 3×2（カーブは
  normal / contrast）。UI は初期選択を明示的に渡すため、コアの
  省略時デフォルト（standard × normal）は不変
- 局所コントラストは ON/OFF トグル（ON = 0.5）で渡し、
  上級者設定の強さが入ればその値を優先する
- リリースは appVersion 0.2.0（タグ v0.2.0）とする

## 8. テスト計画

- 既存のゴールデンテスト・参照テストが**無変更で**全て通ること
  （既定値のバイト一致）
- 新規単体テスト：
  - 合計 1 の重みで無彩色が standard と一致する
  - 範囲外の重み（例 (2, −1, 0) × 純赤）でも出力が 0〜1 に収まる
  - トーンカーブ：アンカー点で定義値と一致、単調非減少、
    normal が恒等
  - 局所コントラスト：amount = 0 で恒等、一様画像で恒等
    （偏差ゼロ）、既知の小パターンで増幅方向が正しい、
    端の窓が有効画素数で正規化される（手計算と一致）
  - 名前解決：未知の名前がエラーになる
- CLI と WASM で同一パラメタの結果が一致すること（スモークテスト）

## 9. 実装フェーズ

1. **フェーズ1：コア＋CLI**（＋テスト）。要件 §8 の印刷実験は
   CLI が一番速く回せるため、まずここまでで実験可能にする
2. **フェーズ2：WASM＋Web UI**。実験で確定したプリセット値を載せる

## 10. 設計判断の記録

- 補間は区分線形（滑らかな補間は実験後に再検討）
- ぼかしの境界は有効領域で切り取り実画素数で割る（既存処理と同一規則）
- WASM は旧 `convert_rgba` を残し、`convert_rgba_adjusted` を追加する
- 実験第一報（2026-11-16）：warm × contrast が人物・紅葉で好評。
  lift-blacks は全画像で不評のため、フェーズ2の Web プリセットには
  含めない方向（コアの ToneCurve には残す）
- 実験第二報（2026-11-16）：contrast が風景・イラスト・人物でも
  支持され、全評価で優位。フェーズ2の推奨候補は warm × contrast
- 既定の確定（2026-11-16）：Web の初期選択を warm × contrast とし、
  選択肢は 3×2（standard / green × normal も選択可能）。
  局所コントラストは選択欄の ON/OFF（ON = 0.5）とする
- 局所コントラストの半径規則と増幅定数（0.15）は設計値であり、
  印刷実験で動かす対象。変える場合は本書を更新する
- 利用者向け表示名は未決（要件 §10）。内部名は本書の英語名を使う
