# rs-xorriso

A Rust-native tribute to [GNU xorriso](https://www.gnu.org/software/xorriso/).
GNU xorrisoへの敬意を込めたRust版。

This is an early-stage reimplementation, **not** a drop-in replacement.
Feature parity with xorriso is a long-term goal, not a current fact.
これは開発初期段階のリライトであり、xorrisoの完全な代替では**ありません**。
機能の完全一致は長期的な目標であり、現時点での実態ではありません。

## What works today / 現在動く機能

- Writing a flat (no subdirectories) ISO9660 Level 1 image from a
  directory of files, with filenames sanitized to 8.3 format.
  サブディレクトリの無いフラットなディレクトリから、ファイル名を
  8.3形式に変換したISO9660 Level 1イメージを生成できます。
- Verified: the generated image mounts and reads correctly on Windows
  (`Mount-DiskImage`) with file contents intact.
  検証済み: 生成したイメージはWindowsの`Mount-DiskImage`で実際に
  マウント・ファイル内容の読み出しができることを確認しています。

```bash
rs-xorriso -as mkisofs -V MY_LABEL -o output.iso ./source_dir
```

## What does not work yet / まだ動かない機能

- Subdirectories (single flat directory only)
- Rock Ridge / Joliet extensions (long filenames, non-ASCII names)
- Actual disc burning (this tool only writes ISO images)
- Multi-sector root directories (large file counts)

サブディレクトリ・Rock Ridge/Joliet拡張(長いファイル名・非ASCII名)・
実際のディスク書き込み(本ツールはISOイメージの生成のみ)・
多数ファイルによる複数セクタのルートディレクトリは、いずれも未対応です。

## Development / 開発

```bash
cargo test
cargo build --release
```

## License

MIT
