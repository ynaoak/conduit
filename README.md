# conduit

キーボードランチャー（Windows / macOS / Linux）。ホットキーで呼び出し、アプリ起動・ファイル検索・ウィンドウ切替・クリップボード履歴・ポートを掴んだプロセスの終了までを、数文字と Enter で実行できます。Windows では Ctrl 二連打でも呼び出せます。

A keyboard launcher for Windows, macOS and Linux. Summon it with a hotkey (double-tap Ctrl on Windows) to launch apps, find files, switch windows, restore clipboard history, and kill the process holding a TCP port — a few keystrokes and Enter.

## ダウンロード / Download

[**Releases**](https://github.com/ynaoak/conduit/releases/latest) から入手できます:

| OS | ファイル | 用途 |
|---|---|---|
| Windows | `*-setup.exe` | インストーラ（推奨 / recommended） |
| Windows | `*.msi` | MSI インストーラ |
| Windows | `*portable.zip` | インストール不要のポータブル版 |
| macOS | `*_universal.dmg` | Apple Silicon / Intel 両対応 |
| Linux | `*.AppImage` | 単一ファイル（自動アップデート対応） |
| Linux | `*.deb` / `*.rpm` | apt / dnf で管理 |

macOS / Linux のファイルはリリース公開の約 30 分後に CI が添付します。
macOS / Linux files are attached by CI within ~30 minutes of a release
appearing.

## ドキュメント / Documentation

- 使い方・ワークフロー（manifest.json）仕様・設定リファレンス: https://conduit.ynaoak.dev/info/ （[English](https://conduit.ynaoak.dev/en/info/)）

## ソースからビルド / Build from source

前提: Rust (stable), Node.js, pnpm（Linux は WebKitGTK 4.1 ほか
[Tauri の必須パッケージ](https://tauri.app/start/prerequisites/)も）

```
git clone https://github.com/ynaoak/conduit
cd conduit
pnpm install
pnpm tauri build
```

実行ファイルは `build/desktop/target/release/` 以下に生成されます。

## License

See [LICENSE](LICENSE).
