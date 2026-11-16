# 色調整 印刷実験セット（v0.2 フェーズ1）

`docs/adjustment-requirements.md` §8 の印刷実験のための一式。
試験画像はすべて AI 生成の写真4点と描画イラスト1点（実在の人物・場所ではない）。

## 試験画像（test-images/）

| ファイル | 内容 | 主な確認点 |
|---|---|---|
| portrait.jpg | 人物：肌色＋赤いマフラー＋緑のコート | 赤と緑の分離、肌の階調 |
| landscape.jpg | 風景：緑の山、青い川と空、オレンジ屋根の家 | 緑の階調、青の扱い |
| warm.jpg | 暖色被写体：紅葉（赤・オレンジ・黄） | 暖色同士の分離 |
| texture.jpg | 質感：ニットの編み目と陰影 | 局所コントラスト、表面の質感 |
| illustration.png | イラスト・線画：平色＋黒い輪郭線 | 線の保持、平色の分離 |

## 出力（outputs/<画像名>/）

幅 384 ドット（T02 相当）で生成：

- `<mixer>-<curve>.png`：ミキサー3種（standard / warm / green）×
  トーンカーブ3種（normal / contrast / lift-blacks）の9通り
- `local-0.5.png`：standard × normal に局所コントラスト 0.5 を追加したもの

再生成コマンドの例：

```
pic2gray4px test-images/warm.jpg out.png --width 384 --mixer warm --tone-curve contrast
pic2gray4px test-images/texture.jpg out.png --width 384 --local-contrast 0.5
```

## コンタクトシート（sheets/）

各画像1枚。上段は原画（カラー）、現行既定（standard × normal）、
局所コントラスト追加版。下段の3×3が9通り（行＝ミキサー、列＝トーンカーブ）。

## 判定について

最終判定は実印刷で行う（要件 §8）。画面上の見た目は参考。
候補を絞ったら幅 576（M02S 相当）で再生成して再試験する。
