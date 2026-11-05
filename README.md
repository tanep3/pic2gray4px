# pic2gray4px

白黒なのに、写真が見える。

`pic2gray4px` は、2×2 の小さなマスに白を何個置くかで 5 段階の明るさを作る Rust 製 CLI です。白点の位置は周囲の見え方を見ながら選びます。さらに、マスごとに丸めたときの端数を隣のマスへ渡すので、広いグラデーションの濃さも残しやすくなりました。できあがるのは、一般的なビューアで開ける **1-bit グレースケール PNG** です。

## 変換前と変換後

| 変換前（カラー） | 変換後（1-bit グレースケール PNG） |
| --- | --- |
| ![変換前のたね子](docs/images/taneko-before.png) | ![4px グレーに変換したたね子](docs/images/taneko-after.png) |

どちらも同じ正方形の構図で、384×384 ピクセルです。変換前は元写真を縮小したカラー画像、変換後は元写真を CLI の `--width 384` で直接変換した画像です。縮小の処理方法が異なるため、画素単位で同じ入力を比較したものではありません。

## インストール

プロジェクトのルートで:

```sh
cargo install --path .
```

## 使ってみる

```sh
pic2gray4px photo.jpg
# photo-4px.png ができる
```

T02 など幅 384 ピクセル向けなら:

```sh
pic2gray4px photo.jpg photo-t02.png --width 384
```

入力は PNG、JPEG、WebP、BMP、TIFF、PNM に対応します。画像が奇数寸法なら右端・下端を白で補います。透過部分は白地に重ねます。

既存の出力ファイルは上書きしません。意図して上書きするときだけ `--force` を付けてください。

## 写りを調整する

標準では画像ごとに控えめなコントラストとアンシャープを選び、**実際に選んだ値を実行結果に表示**します。選択は飽和画素の増加と明るさの変化量を制限する簡易推定です。見た目の最良値を保証するものではありません。

```sh
pic2gray4px photo.jpg --contrast 1.08 --unsharp 0.25
pic2gray4px photo.jpg another.png --contrast off --unsharp off
```

`--contrast` は `auto`（標準）、`off`（倍率 1）、または 0〜3。`--unsharp` は `auto`（標準）、`off`（強さ 0）、または 0〜2。強い値では白飛び、黒つぶれ、輪郭の不自然さが出ることがあります。

元の VM 実験の「マスごとに白点数を独立に丸める」方式と比べるなら、`--no-diffusion --contrast off --unsharp off` を指定します。`--passes 0..10` は配置を見直す上限回数で、標準は 10 です。

詳しくは [要件定義](docs/requirements.md)、[アルゴリズム](docs/algorithm.md)、[システム設計](docs/design.md) を参照してください。

Copyright (c) 2026 Tane Channel Technology. MIT License.
