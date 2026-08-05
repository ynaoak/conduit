# conduit

Windows 向けのキーボードランチャー。Ctrl 二連打で呼び出し、アプリ起動・ファイル検索・ウィンドウ切替・クリップボード履歴・ポートを掴んだプロセスの終了までを、数文字と Enter で実行できます。

A keyboard launcher for Windows. Double-tap Ctrl to launch apps, find files, switch windows, restore clipboard history, and kill the process holding a TCP port — a few keystrokes and Enter.

## ダウンロード / Download

[**Releases**](https://github.com/ynaoak/conduit/releases/latest) から入手できます:

| ファイル | 用途 |
|---|---|
| `*-setup.exe` | インストーラ（推奨 / recommended） |
| `*.msi` | MSI インストーラ |
| `*portable.zip` | インストール不要のポータブル版 |

## ドキュメント / Documentation

- 使い方・ワークフロー（manifest.json）仕様・設定リファレンス: https://conduit.ynaoak.dev/info/ （[English](https://conduit.ynaoak.dev/en/info/)）

## ソースからビルド / Build from source

前提: Rust (stable), Node.js, pnpm, Windows 10/11

```
git clone https://github.com/ynaoak/conduit
cd conduit
pnpm install
pnpm tauri build
```

`src-tauri/target/release/conduit.exe` が生成されます。

## License

See [LICENSE](LICENSE).
